//! Deterministic bounded native regression corpus. See terminal-corpus.md.
#[cfg(feature = "ghostty")]
#[path = "terminal_corpus_support/runner.rs"]
mod runner;
fn main() {
    #[cfg(feature = "ghostty")]
    runner::run();
    #[cfg(not(feature = "ghostty"))]
    {
        eprintln!("terminal_corpus requires --features ghostty");
        std::process::exit(2);
    }
}
