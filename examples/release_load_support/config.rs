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
    /// Parse the fixture's own arguments. `args` excludes argv[0].
    ///
    /// Every token is consumed as a recognised option or as that option's
    /// value, and anything left over is refused. A lenient parse is how a
    /// measurement silently becomes a different measurement: a four-point
    /// staging-slot sweep was run at four settings and all four were the
    /// default, because the driver sent `--staging_slots` and a
    /// scan-for-known-names parser simply did not find it.
    ///
    /// Scanning for `--` prefixes alone was not enough either. `release_load
    /// staging-slots 16` has no unknown `--` token, so it ran at depth 256
    /// while looking like a depth-16 run, and `--raw false` enabled raw mode
    /// and dropped the `false` on the floor.
    pub fn parse(args: &[String]) -> Result<Self> {
        // Names that take exactly one value, and names that take none.
        const VALUED: [&str; 12] = [
            "--mode",
            "--sessions",
            "--active",
            "--seconds",
            "--warmup",
            "--rate",
            "--producer-bytes",
            "--staging-slots",
            "--chunk",
            "--observers",
            "--cols",
            "--rows",
        ];
        const FLAGS: [&str; 1] = ["--raw"];
        let refuse = |message: String| -> Box<dyn std::error::Error + Send + Sync> {
            std::io::Error::other(message).into()
        };
        let mut given: Vec<(&str, &str)> = Vec::new();
        let mut raw = false;
        let mut index = 0;
        while index < args.len() {
            let argument = args[index].as_str();
            if let Some(name) = VALUED.iter().find(|name| **name == argument) {
                let Some(value) = args.get(index + 1) else {
                    return Err(refuse(format!(
                        "{name} was given without a value; the run would otherwise \
                         silently use the default"
                    )));
                };
                if given.iter().any(|(seen, _)| seen == name) {
                    return Err(refuse(format!(
                        "{name} was given more than once; which one describes the \
                         run cannot be decided here"
                    )));
                }
                given.push((name, value.as_str()));
                index += 2;
            } else if FLAGS.contains(&argument) {
                if raw {
                    return Err(refuse("--raw was given more than once".to_owned()));
                }
                raw = true;
                index += 1;
            } else {
                return Err(refuse(format!(
                    "unrecognised fixture argument {argument}; a run that silently used \
                     defaults instead would not be the run that was requested"
                )));
            }
        }
        let value = |name: &str, fallback: &str| -> String {
            given
                .iter()
                .find(|(key, _)| *key == name)
                .map_or_else(|| fallback.to_owned(), |(_, value)| (*value).to_owned())
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
            raw,
        };
        if result.sessions == 0
            // ADR 0002 asks for 500 sessions qualified "on a host with sufficient
            // PTY/process capacity", and notes the macOS host's system-wide PTY
            // limit of 511 as the reason that is not everywhere. The cap is the
            // fixture's own, so it has to allow what the ADR asks for; whether a
            // given host can actually carry it is what the run finds out.
            || result.sessions > 512
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
                    | "detaching"
                    | "reference"
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
        // These modes are *about* what an observer sees, so running one without
        // an observer exercises nothing it claims to. `detaching` is the case
        // that mattered: its drop is guarded on there being attachments to drop,
        // so with none it emitted no `observers_detached` record, crossed no
        // transition, and still passed — a trial labelled as testing the detach
        // transition, proving that the transition was not tested.
        if result.observers == 0
            && matches!(
                result.mode.as_str(),
                "attached"
                    | "detaching"
                    | "reference"
                    | "dominant"
                    | "stalled-observer"
                    | "stalled-sink"
            )
        {
            return Err(std::io::Error::other(format!(
                "{} is a claim about what an observer sees and needs at least one",
                result.mode
            ))
            .into());
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

    /// A misspelled flag must fail rather than quietly measure the default.
    #[test]
    fn an_unrecognised_argument_is_refused_rather_than_ignored() {
        assert!(
            parse(&["--staging_slots", "16"]).is_err(),
            "an underscore spelling must not silently run at the default"
        );
        assert!(parse(&["--stagingslots", "16"]).is_err());
        assert!(parse(&["--unknown"]).is_err());
        // The values themselves are not names and must stay unaffected.
        assert!(parse(&["--mode", "saturation", "--rate", "0"]).is_ok());
    }

    /// Checking only for unknown `--` tokens left the same silent-default hole
    /// open one step further along: a token that is not an option at all, and a
    /// value handed to an option that takes none.
    #[test]
    fn a_token_that_is_never_consumed_is_refused() {
        assert!(
            parse(&["staging-slots", "16"]).is_err(),
            "a missing `--` must not run at the default while looking like a sweep point"
        );
        assert!(
            parse(&["--raw", "false"]).is_err(),
            "`--raw false` must not enable raw mode and discard the word that denied it"
        );
        assert!(parse(&["--mode"]).is_err(), "a name with no value");
        assert!(parse(&["extra"]).is_err());
        assert!(
            parse(&["--seconds", "60", "--seconds", "3600"]).is_err(),
            "two durations describe two different runs"
        );
        // argv[0] is dropped by the caller, so a path is a stray token here.
        assert!(parse(&["target/release/examples/release_load"]).is_err());
        assert!(parse(&["--raw"]).unwrap().raw);
        assert!(!parse(&[]).unwrap().raw);
    }

    /// A mode named for an observer must have one, or it proves nothing.
    #[test]
    fn an_observer_mode_without_an_observer_is_refused() {
        for mode in [
            "attached",
            "detaching",
            "reference",
            "dominant",
            "stalled-observer",
            "stalled-sink",
        ] {
            assert!(
                parse(&["--mode", mode, "--observers", "0"]).is_err(),
                "{mode} with no observer exercises nothing it is named for"
            );
            assert!(
                parse(&["--mode", mode, "--observers", "1"]).is_ok(),
                "{mode}"
            );
        }
        // `detached` forces zero attachments at run time and is the deliberate
        // opposite case; the idle and resource cases have no observer by
        // design.
        assert!(parse(&["--mode", "detached", "--observers", "0"]).is_ok());
        assert!(
            parse(&[
                "--mode",
                "idle",
                "--active",
                "0",
                "--rate",
                "0",
                "--observers",
                "0"
            ])
            .is_ok()
        );
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
