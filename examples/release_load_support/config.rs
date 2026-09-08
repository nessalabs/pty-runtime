use super::Result;
#[derive(Debug)]
pub struct Config {
    pub mode: String,
    pub sessions: usize,
    pub active: usize,
    pub seconds: u64,
    pub warmup: u64,
    pub rate: u64,
    pub chunk: usize,
    pub observers: usize,
    pub cols: u16,
    pub rows: u16,
    pub raw: bool,
}
impl Config {
    pub fn parse(args: &[String]) -> Result<Self> {
        let value = |name: &str, fallback: &str| -> String {
            args.windows(2)
                .find(|pair| pair[0] == name)
                .map_or_else(|| fallback.to_owned(), |pair| pair[1].clone())
        };
        let result = Self {
            mode: value("--mode", "attached"),
            sessions: value("--sessions", "64").parse()?,
            active: value("--active", "16").parse()?,
            seconds: value("--seconds", "60").parse()?,
            warmup: value("--warmup", "10").parse()?,
            rate: value("--rate", "10485760").parse()?,
            chunk: value("--chunk", "4093").parse()?,
            observers: value("--observers", "1").parse()?,
            cols: value("--cols", "80").parse()?,
            rows: value("--rows", "24").parse()?,
            raw: args.iter().any(|arg| arg == "--raw"),
        };
        if result.sessions == 0
            || result.sessions > 128
            || result.active > result.sessions
            || result.seconds == 0
            || result.chunk == 0
            || result.chunk > 65536
            || result.observers > 16
            || !matches!(
                result.mode.as_str(),
                "attached" | "detached" | "stalled-observer" | "stalled-sink" | "dominant" | "idle"
            )
        {
            return Err(std::io::Error::other("invalid bounded load configuration").into());
        }
        if (result.mode == "idle" && result.active != 0) || (result.active == 0 && result.rate != 0)
        {
            return Err(std::io::Error::other("idle requires --active 0 --rate 0").into());
        }
        Ok(result)
    }
    pub fn rate_for(&self, index: usize) -> u64 {
        if index >= self.active || self.active == 0 {
            0
        } else if self.mode == "dominant" && self.active > 1 {
            if index == 0 {
                self.rate * 9 / 10
            } else {
                self.rate / 10 / (self.active - 1) as u64
            }
        } else {
            self.rate / self.active as u64
        }
    }
}
