use std::io::{Read, Write};
pub fn query() {
    super::raw();
    std::io::stdout()
        .write_all(b"\x1b[2J\x1b[Habc\x1b[6n")
        .unwrap();
    std::io::stdout().flush().unwrap();
    let mut reply = [0; 6];
    std::io::stdin().read_exact(&mut reply).unwrap();
    assert_eq!(&reply, b"\x1b[1;4R");
    std::io::stdout().write_all(b"REPLY_OK").unwrap();
    std::io::stdout().flush().unwrap();
    let mut finish = [0; 4];
    std::io::stdin().read_exact(&mut finish).unwrap();
    assert_eq!(
        &finish, b"done",
        "a second reply must not precede user input"
    );
    std::io::stdout().write_all(b"DONE").unwrap();
}
