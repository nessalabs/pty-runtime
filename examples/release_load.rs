//! Independent PTY producers and exact bounded replay accounting for release qualification.
mod release_load_support;
use release_load_support::{Result, allocator, config::Config, fixture, run};
#[global_allocator]
static ALLOCATOR: allocator::Tracking = allocator::Tracking;
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--child") {
        return fixture::run(&args[2], args[3].parse()?);
    }
    // argv[0] is a program path, not an option, and the parser refuses every
    // token it cannot consume — so it is dropped here rather than special-cased
    // inside the parse.
    let config = Config::parse(&args[1..])?;
    if !cfg!(feature = "ghostty") && !config.raw {
        return Err(std::io::Error::other("projected load requires Ghostty feature").into());
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run::execute(config))
}
