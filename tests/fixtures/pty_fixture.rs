//! Deterministic local process fixture; only synthetic input/output.
use std::io::{Read, Write};
fn raw() {
    assert!(
        std::process::Command::new("/bin/stty")
            .args(["raw", "-echo"])
            .status()
            .unwrap()
            .success()
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("bytes") => {
            raw();
            let length: usize = args[2].parse().unwrap();
            let mut out = std::io::stdout().lock();
            for i in 0..length {
                out.write_all(&[(i % 251) as u8]).unwrap();
            }
        }
        Some("prompt") => {
            print!("synthetic-code: ");
            std::io::stdout().flush().unwrap();
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            println!("accepted:{}", input.trim().len());
        }
        Some("hold") => {
            println!("ready");
            loop {
                std::thread::sleep(std::time::Duration::from_secs(60));
            }
        }
        Some("echo") => {
            raw();
            print!("ready");
            std::io::stdout().flush().unwrap();
            let mut bytes = [0; 256];
            loop {
                let n = std::io::stdin().read(&mut bytes).unwrap();
                if n == 0 {
                    break;
                }
                std::io::stdout().write_all(&bytes[..n]).unwrap();
                std::io::stdout().flush().unwrap();
            }
        }
        Some("flood") => {
            raw();
            let data = [b'x'; 4096];
            let mut out = std::io::stdout().lock();
            loop {
                if out.write_all(&data).is_err() {
                    break;
                }
            }
        }
        Some("exit") => std::process::exit(args[2].parse().unwrap()),
        _ => std::process::exit(64),
    }
}
