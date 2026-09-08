//! Build/stage the helper for Cargo's TARGET, independently of the host workspace.
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
};

pub fn build(root: &Path, out: &Path) -> io::Result<()> {
    println!("cargo:rerun-if-env-changed=PTY_RUNTIME_GUARDIAN_IMAGE");
    for path in [
        "helpers/guardian/Cargo.toml",
        "helpers/guardian/Cargo.lock",
        "helpers/guardian/src",
        "scripts/guardian/protocol.rs",
        "scripts/guardian/build.rs",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(path).display());
    }
    let target = env::var("TARGET").map_err(|_| io::ErrorKind::InvalidInput)?;
    let image = if let Some(image) = env::var_os("PTY_RUNTIME_GUARDIAN_IMAGE") {
        let image = PathBuf::from(image);
        println!("cargo:rerun-if-changed={}", image.display());
        image
    } else {
        let target_dir = out.join("guardian-target");
        let cargo = env::var_os("CARGO").ok_or(io::ErrorKind::InvalidInput)?;
        let status = Command::new(cargo)
            .arg("build")
            .arg("--locked")
            .arg("--release")
            .arg("--manifest-path")
            .arg(root.join("helpers/guardian/Cargo.toml"))
            .arg("--target")
            .arg(&target)
            .arg("--target-dir")
            .arg(&target_dir)
            .arg("--bin")
            .arg("pty-runtime-guardian")
            .status()?;
        if !status.success() {
            return Err(io::Error::other("target guardian build failed"));
        }
        target_dir
            .join(&target)
            .join("release/pty-runtime-guardian")
    };
    let bytes = fs::read(image)?;
    validate_target(&bytes, &target)?;
    let staged = out.join("pty-runtime-guardian-image");
    fs::write(&staged, bytes)?;
    println!(
        "cargo:rustc-env=PTY_RUNTIME_GUARDIAN_IMAGE_PATH={}",
        staged.display()
    );
    Ok(())
}
fn validate_target(bytes: &[u8], target: &str) -> io::Result<()> {
    let valid = if target.contains("apple-darwin") && bytes.len() >= 8 {
        let cpu = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        bytes[..4] == [0xcf, 0xfa, 0xed, 0xfe]
            && ((target.starts_with("aarch64-") && cpu == 0x0100000c)
                || (target.starts_with("x86_64-") && cpu == 0x01000007))
    } else if target.contains("linux") && bytes.len() >= 20 {
        let machine = u16::from_le_bytes([bytes[18], bytes[19]]);
        bytes[..4] == *b"\x7fELF"
            && bytes[4..6] == [2, 1]
            && ((target.starts_with("aarch64-") && machine == 183)
                || (target.starts_with("x86_64-") && machine == 62))
    } else {
        false
    };
    if valid {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "guardian image does not match TARGET",
        ))
    }
}
