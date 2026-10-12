use std::io::{Read, Write};

const QUERY: &[u8] = b"\x1b]7501;?\x1b\\";

pub fn query(projected: bool) {
    super::raw();
    let reports = b"\x1b]7501;state=idle:app=fixture\x07\x1b]7501;state=working:id=job:progress=25\x1b\\\x1b]7501;state=blocked:id=permission:kind=permission\x07\x1b]7501;state=done:id=finished\x07\x1b]7501;state=error:id=failed\x07";
    std::io::stdout().write_all(reports).unwrap();
    std::io::stdout().write_all(QUERY).unwrap();
    if projected {
        std::io::stdout().write_all(b"\x1b[c").unwrap();
    }
    std::io::stdout().flush().unwrap();
    if projected {
        let mut reply = [0; 10];
        std::io::stdin().read_exact(&mut reply).unwrap();
        assert_eq!(&reply, QUERY, "minimal protocol reply must precede DA1");
        let mut da1 = Vec::new();
        loop {
            let mut byte = [0];
            std::io::stdin().read_exact(&mut byte).unwrap();
            da1.push(byte[0]);
            assert!(da1.len() <= 128, "bounded device attributes reply");
            if byte[0] == b'c' {
                break;
            }
        }
        assert!(da1.starts_with(b"\x1b[?"));
        assert!(
            da1[3..da1.len() - 1]
                .iter()
                .all(|b| b.is_ascii_digit() || *b == b';')
        );
    }
    std::io::stdout().write_all(b"STATUS_READY").unwrap();
    std::io::stdout().flush().unwrap();
    let mut finish = [0; 4];
    std::io::stdin().read_exact(&mut finish).unwrap();
    assert_eq!(&finish, b"done", "no unsolicited or duplicate replies");
    std::io::stdout()
        .write_all(b"\x1b]7501;state=working:id=tail\x07STATUS_EXIT")
        .unwrap();
}
