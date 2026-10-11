use super::report::{Mutation, parse};
#[derive(Clone, Default)]
pub(super) struct Scanner {
    mode: Mode,
    body: Vec<u8>,
    length: usize,
    escaped: bool,
}
#[derive(Clone, Copy, Default)]
enum Mode {
    #[default]
    Ground,
    Escape,
    Osc,
    String,
}
impl Scanner {
    pub(super) fn retained_bytes(&self) -> usize {
        self.body.capacity()
    }
    pub(super) fn scan(
        &mut self,
        input: &[u8],
        mutations: &mut Vec<Mutation>,
        queries: &mut Vec<usize>,
    ) {
        let mut offset = 0;
        while offset < input.len() {
            if matches!(self.mode, Mode::Ground) {
                let Some(next) = input[offset..].iter().position(|b| *b == 0x1b) else {
                    break;
                };
                offset += next;
            }
            let byte = input[offset];
            offset += 1;
            match self.mode {
                Mode::Ground => self.mode = Mode::Escape,
                Mode::Escape => match byte {
                    b']' => {
                        self.mode = Mode::Osc;
                        self.length = 2;
                        self.body.clear();
                        self.escaped = false;
                    }
                    b'P' | b'X' | b'^' | b'_' => {
                        self.mode = Mode::String;
                        self.escaped = false;
                    }
                    b'c' => {
                        mutations.push(Mutation::Reset);
                        self.mode = Mode::Ground
                    }
                    0x1b => {}
                    _ => self.mode = Mode::Ground,
                },
                Mode::Osc | Mode::String => {
                    if matches!(byte, 0x18 | 0x1a) {
                        self.mode = Mode::Ground;
                        self.body.clear();
                        continue;
                    }
                    let osc = matches!(self.mode, Mode::Osc);
                    self.length = self.length.saturating_add(1);
                    if (self.escaped && byte == b'\\') || (osc && byte == 7) {
                        if osc && self.length <= 4096 {
                            if self.escaped && byte == b'\\' {
                                self.body.pop();
                            }
                            if let Some(body) = self.body.strip_prefix(b"7501;") {
                                if body == b"?" {
                                    queries.push(offset)
                                } else if let Some(m) = parse(body) {
                                    mutations.push(m)
                                }
                            } else if self
                                .body
                                .strip_prefix(b"133;")
                                .is_some_and(|b| b.split(|v| *v == b';').next() == Some(b"A"))
                            {
                                mutations.push(Mutation::Exit)
                            }
                        }
                        self.body.clear();
                        self.mode = Mode::Ground;
                        self.escaped = false;
                    } else {
                        if osc && self.length <= 4096 {
                            self.body.push(byte)
                        }
                        self.escaped = byte == 0x1b;
                    }
                }
            }
        }
    }
}
