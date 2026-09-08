use super::Result;
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
};
pub fn run(path: &str, seed: u64) -> Result<()> {
    if !std::process::Command::new("/bin/stty")
        .args(["raw", "-echo"])
        .status()?
        .success()
    {
        return Err(std::io::Error::other("fixture could not configure raw PTY").into());
    }
    let mut control = UnixStream::connect(path)?;
    control.write_all(b"r")?;
    let mut offset = 0u64;
    let mut command = [0];
    while control.read_exact(&mut command).is_ok() {
        match command[0] {
            b'x' => return Ok(()),
            b'b' => {
                let bytes: Vec<u8> = (0..4096)
                    .map(|index| super::pattern(seed, offset + index))
                    .collect();
                std::io::stdout().write_all(&bytes)?;
                std::io::stdout().flush()?;
                offset += bytes.len() as u64;
                control.write_all(b"a")?;
            }
            _ => return Err(std::io::Error::other("invalid synthetic fixture command").into()),
        }
    }
    Ok(())
}
