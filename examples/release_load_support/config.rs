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
    pub producer_bytes: u64,
    pub staging_slots: usize,
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
            producer_bytes: value("--producer-bytes", "8589934592").parse()?,
            staging_slots: value("--staging-slots", "256").parse()?,
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
            || result.producer_bytes == 0
            || result.producer_bytes > 68_719_476_736
            || result.staging_slots == 0
            || result.staging_slots > 1_048_576
            || result.chunk == 0
            || result.chunk > 65536
            || result.observers > 16
            || !matches!(
                result.mode.as_str(),
                "attached"
                    | "detached"
                    | "stalled-observer"
                    | "stalled-sink"
                    | "dominant"
                    | "idle"
                    | "saturation"
            )
        {
            return Err(std::io::Error::other("invalid bounded load configuration").into());
        }
        if (result.mode == "idle" && result.active != 0) || (result.active == 0 && result.rate != 0)
        {
            return Err(std::io::Error::other("idle requires --active 0 --rate 0").into());
        }
        if result.mode == "saturation" && (result.active == 0 || result.rate != 0) {
            return Err(
                std::io::Error::other("saturation requires active producers and --rate 0").into(),
            );
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

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(arguments: &[&str]) -> Result<Config> {
        Config::parse(
            &arguments
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<Vec<_>>(),
        )
    }
    #[test]
    fn saturation_is_explicit_and_output_budget_is_finite() {
        assert!(parse(&["--mode", "saturation", "--rate", "0"]).is_ok());
        assert!(parse(&["--mode", "saturation"]).is_err());
        assert!(parse(&["--producer-bytes", "0"]).is_err());
        assert!(parse(&["--mode", "saturation", "--rate", "0", "--active", "0"]).is_err());
    }
    #[test]
    fn parser_slot_override_rejects_unbounded_configuration() {
        for value in ["0", "1048577", "-1", "invalid"] {
            assert!(
                parse(&["--staging-slots", value]).is_err(),
                "invalid slot override: {value}"
            );
        }
        assert!(parse(&["--staging-slots", "32"]).is_ok());
    }

    #[test]
    fn parser_slot_override_retains_default_and_requested_finite_budget() {
        assert_eq!(parse(&[]).unwrap().staging_slots, 256);
        for value in [1, 32, 64, 128, 256, 1_048_576] {
            assert_eq!(
                parse(&["--staging-slots", &value.to_string()])
                    .unwrap()
                    .staging_slots,
                value
            );
        }
    }
}
