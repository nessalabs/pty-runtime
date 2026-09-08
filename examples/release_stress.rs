//! Release qualification driver. Defaults are full counts; `--smoke` is not acceptance.
mod release_support;
use release_support::{Result, cycles, fixture, soak};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--child") {
        return fixture::run(&args[2], args[3].parse()?);
    }
    let smoke = args.iter().any(|value| {
        value == "--smoke" || value == "--parking-smoke" || value == "--restore-smoke"
    });
    let mode = args.get(1).map(String::as_str).unwrap_or("repetition");
    let started = std::time::Instant::now();
    println!(
        "{{\"event\":\"start\",\"mode\":\"{mode}\",\"smoke\":{smoke},\"pid\":{}}}",
        std::process::id()
    );
    match mode {
        "repetition" => {
            cycles::run(
                if smoke { 12 } else { 10_000 },
                if smoke { 100 } else { 100_000 },
            )
            .await?
        }
        "races" => cycles::races(if smoke { 4 } else { 256 }, 0x52c9_117a_63bd_04e1).await?,
        "soak" => {
            soak::run(if args.iter().any(|value| value == "--restore-smoke") {
                270
            } else if args.iter().any(|value| value == "--parking-smoke") {
                85
            } else if smoke {
                5
            } else {
                12 * 60 * 60
            })
            .await?
        }
        _ => {
            return Err(
                std::io::Error::other("use repetition, races, or soak; optional --smoke").into(),
            );
        }
    }
    println!(
        "{{\"event\":\"complete\",\"mode\":\"{mode}\",\"smoke\":{smoke},\"seconds\":{}}}",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}
