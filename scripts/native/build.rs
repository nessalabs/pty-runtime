//! Shared build-script body for the infrastructure native terminal feature.
use std::{env, path::PathBuf, process::Command};
#[path = "../guardian/build.rs"]
mod guardian_image;
fn checked(command: &mut Command) {
    let status = command.status().expect("native compiler could not start");
    assert!(status.success(), "native compiler failed");
}
fn main() {
    let root =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory")).join("../..");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("output directory"));
    if let Err(error) = guardian_image::build(&root, &out) {
        eprintln!("guardian image build failed: {error}");
        std::process::exit(1);
    }
    println!("cargo:rerun-if-env-changed=PTY_RUNTIME_GHOSTTY_SOURCE");
    if env::var_os("CARGO_FEATURE_GHOSTTY").is_none() {
        return;
    }
    let source = env::var_os("PTY_RUNTIME_GHOSTTY_SOURCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            root.join("work/experiment-cache/ghostty-82232ecde55405559dec29c5466cb9e39938cb41")
        });
    checked(
        Command::new("python3")
            .arg(root.join("scripts/native/verify_source.py"))
            .arg(&source)
            .arg("--built"),
    );
    for name in ["verify_source.py", "patches/snapshot-pending-wrap.patch"] {
        println!(
            "cargo:rerun-if-changed={}",
            root.join("scripts/native").join(name).display()
        );
    }
    println!(
        "cargo:rerun-if-changed={}",
        source.join("experiment-build.json").display()
    );
    let mut objects = Vec::new();
    for name in ["owner", "checkpoint", "view", "verification"] {
        let file = root.join(format!("scripts/native/{name}.c"));
        println!("cargo:rerun-if-changed={}", file.display());
        let object = out.join(format!("{name}.o"));
        checked(
            Command::new("cc")
                .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-O2", "-c"])
                .arg(file)
                .arg("-I")
                .arg(source.join("include"))
                .arg("-o")
                .arg(&object),
        );
        objects.push(object);
    }
    println!(
        "cargo:rerun-if-changed={}",
        root.join("scripts/native/bridge.h").display()
    );
    checked(
        Command::new("ar")
            .arg("crs")
            .arg(out.join("libruntime_ghostty_bridge.a"))
            .args(objects),
    );
    let library = source.join("zig-out/lib/libghostty-vt.a");
    assert!(
        library.is_file(),
        "Build the pinned Ghostty library using experiments/run.py first"
    );
    println!("cargo:rerun-if-changed={}", library.display());
    println!("cargo:rustc-link-search=native={}", out.display());
    std::fs::copy(&library, out.join("libpty_runtime_ghostty.a"))
        .expect("stage static Ghostty library");
    println!("cargo:rustc-link-lib=static=runtime_ghostty_bridge");
    println!("cargo:rustc-link-lib=static=pty_runtime_ghostty");
}
