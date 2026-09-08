use std::{io::{Read, Write}, os::unix::net::{UnixListener, UnixStream}, time::{Duration, Instant}};
fn main() {
    let path = std::env::temp_dir().join(format!("load-accept-race-{}", std::process::id()));
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let child = std::thread::spawn(move || {
        let mut socket = UnixStream::connect(path).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        socket.write_all(&[b'r'; 64]).unwrap();
    });
    let mut socket = loop { match listener.accept() { Ok((s, _)) => break s, Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(1)), Err(e) => panic!("{e}") } };
    socket.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let started=Instant::now();
    let mut frame=[0;64];
    println!("inherited_read={:?} elapsed_ms={}", socket.read_exact(&mut frame), started.elapsed().as_millis());
    socket.set_nonblocking(false).unwrap();
    let started=Instant::now();
    println!("explicit_blocking_read={:?} elapsed_ms={}", socket.read_exact(&mut frame), started.elapsed().as_millis());
    child.join().unwrap();
    std::fs::remove_file(std::env::temp_dir().join(format!("load-accept-race-{}", std::process::id()))).unwrap();
}
