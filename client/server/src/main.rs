//! Demo server: one WebSocket carries one rendered terminal to a browser.
//!
//! Run with `cargo run` from `client/server`, then open the printed address.
//! The protocol is described in `client/PROTOCOL.md`.
mod session;
mod wire;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::mpsc;
use wire::{ClientMessage, ServerMessage, VERSION};

#[derive(Clone)]
struct AppState {
    shell: String,
    web: Arc<PathBuf>,
    paste_dir: Arc<PathBuf>,
}

#[tokio::main]
async fn main() {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(7749);

    // The page and component are served from the repository rather than
    // embedded, so editing them does not need a rebuild.
    let web = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../web");
    let paste_dir = std::env::temp_dir().join("pty-runtime-paste");
    if let Err(error) = std::fs::create_dir_all(&paste_dir) {
        eprintln!("cannot create paste directory {}: {error}", paste_dir.display());
        std::process::exit(1);
    }
    let state = AppState {
        shell,
        web: Arc::new(web),
        paste_dir: Arc::new(paste_dir),
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/terminal.js", get(component))
        .route("/ws", get(upgrade))
        .route("/paste-file", post(paste_file))
        .with_state(state);

    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = match tokio::net::TcpListener::bind(address).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("cannot bind {address}: {error}");
            std::process::exit(1);
        }
    };
    println!("terminal demo on http://{address}");
    if let Err(error) = axum::serve(listener, app).await {
        eprintln!("server stopped: {error}");
    }
}

async fn index(State(state): State<AppState>) -> Response {
    match tokio::fs::read_to_string(state.web.join("index.html")).await {
        Ok(body) => Html(body).into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("cannot read index.html: {error}"),
        )
            .into_response(),
    }
}

async fn component(State(state): State<AppState>) -> Response {
    match tokio::fs::read_to_string(state.web.join("terminal.js")).await {
        Ok(body) => (
            [(axum::http::header::CONTENT_TYPE, "text/javascript")],
            body,
        )
            .into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("cannot read terminal.js: {error}"),
        )
            .into_response(),
    }
}

async fn upgrade(upgrade: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    upgrade.on_upgrade(move |socket| serve(socket, state))
}

const PASTE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct PasteJson {
    name: Option<String>,
    #[serde(default, rename = "type")]
    media_type: Option<String>,
    data: String,
}

/// Save a clipboard image under a private temp directory and return its path.
/// The browser pastes that absolute path as ordinary terminal text.
async fn paste_file(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    let (bytes, name, media_type) = if content_type.starts_with("application/json") {
        let Ok(payload) = serde_json::from_slice::<PasteJson>(&body) else {
            return (StatusCode::BAD_REQUEST, "invalid paste json").into_response();
        };
        let Ok(bytes) = decode_base64(&payload.data) else {
            return (StatusCode::BAD_REQUEST, "invalid paste base64").into_response();
        };
        (bytes, payload.name, payload.media_type)
    } else if content_type.starts_with("image/") {
        let name = headers
            .get(header::CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .and_then(disposition_filename)
            .map(str::to_owned);
        (body.to_vec(), name, Some(content_type.to_owned()))
    } else {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "send image/* or application/json",
        )
            .into_response();
    };

    if bytes.is_empty() || bytes.len() > PASTE_BYTES {
        return (StatusCode::PAYLOAD_TOO_LARGE, "paste exceeds 8 MiB").into_response();
    }

    let extension = paste_extension(name.as_deref(), media_type.as_deref());
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let path = state.paste_dir.join(format!("paste-{stamp}{extension}"));
    if let Err(error) = tokio::fs::write(&path, &bytes).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("cannot write paste: {error}"),
        )
            .into_response();
    }
    Json(json!({ "path": path.to_string_lossy() })).into_response()
}

fn paste_extension(name: Option<&str>, media_type: Option<&str>) -> &'static str {
    if let Some(name) = name {
        let lower = name.to_ascii_lowercase();
        for (suffix, extension) in [
            (".png", ".png"),
            (".jpg", ".jpg"),
            (".jpeg", ".jpg"),
            (".gif", ".gif"),
            (".webp", ".webp"),
            (".tif", ".tif"),
            (".tiff", ".tif"),
        ] {
            if lower.ends_with(suffix) {
                return extension;
            }
        }
    }
    match media_type.unwrap_or("") {
        "image/jpeg" => ".jpg",
        "image/gif" => ".gif",
        "image/webp" => ".webp",
        "image/tiff" => ".tif",
        _ => ".png",
    }
}

fn disposition_filename(value: &str) -> Option<&str> {
    value.split(';').find_map(|part| {
        let part = part.trim();
        part.strip_prefix("filename=")
            .map(|name| name.trim_matches('"'))
    })
}

fn decode_base64(input: &str) -> Result<Vec<u8>, ()> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [255u8; 256];
    for (index, byte) in TABLE.iter().enumerate() {
        lookup[*byte as usize] = index as u8;
    }
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut accumulator = 0u32;
    let mut bits = 0u32;
    for byte in input.bytes() {
        if byte == b'=' || byte.is_ascii_whitespace() {
            continue;
        }
        let value = lookup[byte as usize];
        if value == 255 {
            return Err(());
        }
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((accumulator >> bits) as u8);
        }
    }
    Ok(out)
}

/// Decode base64 input without pulling in a dependency for one direction of
/// one message. Invalid input yields no bytes rather than an error, because a
/// malformed frame should not take the session down.
fn base64(input: &str) -> Vec<u8> {
    decode_base64(input).unwrap_or_default()
}

async fn serve(mut socket: WebSocket, state: AppState) {
    // The first message must establish the geometry; anything else is a
    // protocol error and the socket is closed rather than guessed at.
    let (cols, rows) = loop {
        match socket.recv().await {
            Some(Ok(Message::Text(text))) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(message) if message.version() != VERSION => {
                    let _ = socket
                        .send(Message::Text(
                            serde_json::to_string(&ServerMessage::Error {
                                v: VERSION,
                                message: format!("unsupported protocol version {}", message.version()),
                            })
                            .unwrap_or_default()
                            .into(),
                        ))
                        .await;
                    return;
                }
                Ok(ClientMessage::Hello { cols, rows, .. }) => break (cols, rows),
                Ok(_) => return,
                Err(_) => return,
            },
            Some(Ok(_)) => continue,
            _ => return,
        }
    };

    let (command_tx, command_rx) = mpsc::channel(64);
    let (frame_tx, mut frame_rx) = mpsc::channel(256);
    let shell = state.shell.clone();
    let worker = std::thread::spawn(move || {
        session::run(shell, cols, rows, command_rx, frame_tx);
    });

    loop {
        tokio::select! {
            outgoing = frame_rx.recv() => {
                let Some(message) = outgoing else { break };
                let terminal = matches!(
                    message,
                    ServerMessage::Exit { .. } | ServerMessage::Error { .. }
                );
                let Ok(text) = serde_json::to_string(&message) else { break };
                if socket.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
                if terminal {
                    break;
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        let Ok(message) = serde_json::from_str::<ClientMessage>(&text) else {
                            continue;
                        };
                        if message.version() != VERSION {
                            break;
                        }
                        let command = match message {
                            ClientMessage::Input { data, .. } => {
                                session::Command::Input(base64(&data))
                            }
                            ClientMessage::Resize { cols, rows, .. } => {
                                session::Command::Resize { cols, rows }
                            }
                            ClientMessage::History { start, count, .. } => {
                                session::Command::History { start, count }
                            }
                            ClientMessage::Hello { .. } => continue,
                        };
                        if command_tx.send(command).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(_)) => continue,
                }
            }
        }
    }

    // Dropping the command channel ends the session loop.
    drop(command_tx);
    let _ = tokio::task::spawn_blocking(move || worker.join()).await;
}
