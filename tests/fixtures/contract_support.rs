//! Synthetic executable-side contract checks; never prints inherited values.
use std::io::{IsTerminal, Write};

pub fn record_launch(path: &str) {
    let mut counter = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    counter.write_all(b"launched\n").unwrap();
    counter.flush().unwrap();
    print!("registered");
    std::io::stdout().flush().unwrap();
    let mut command = String::new();
    std::io::stdin().read_line(&mut command).unwrap();
    std::process::exit(if command == "finish\n" { 23 } else { 94 });
}

fn dimensions() {
    let output = std::process::Command::new("/bin/stty")
        .arg("size")
        .stdin(std::process::Stdio::inherit())
        .output()
        .unwrap();
    if !output.status.success() {
        std::process::exit(93);
    }
    print!("size:");
    std::io::stdout().write_all(&output.stdout).unwrap();
    std::io::stdout().flush().unwrap();
}

pub fn inspect(args: &[String]) {
    let literal = "literal $HOME $(printf injected); *";
    let home_ok = match args[4].as_str() {
        // LLVM coverage startup can add this one key after exec. The public
        // contract independently uses /usr/bin/env to verify exact exec-time emptiness.
        "empty" => std::env::vars_os()
            .all(|(name, _)| name == "PTY_SYNTHETIC_FLAG" || name == "__LLVM_PROFILE_RT_INIT_ONCE"),
        "inherit" => std::env::var("HOME").is_ok_and(|value| value == args[5]),
        _ => false,
    };
    let correct = std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && std::io::stderr().is_terminal()
        && std::fs::File::open("/dev/tty").is_ok()
        && std::env::current_dir().unwrap() == std::fs::canonicalize(&args[2]).unwrap()
        && args[3] == literal
        && home_ok
        && std::env::var_os("PATH").is_none()
        && std::env::var("PTY_SYNTHETIC_FLAG").is_ok_and(|value| value == "final-override");
    if !correct {
        std::process::exit(91);
    }
    print!("contract-ok|");
    dimensions();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    if input != "RESIZE-SYNTHETIC\n" {
        std::process::exit(92);
    }
    print!("resized|");
    dimensions();
    std::process::exit(31);
}
