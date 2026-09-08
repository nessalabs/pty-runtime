unsafe extern "C" { fn getpid() -> i32; fn kill(pid: i32, sig: i32) -> i32; fn _exit(status: i32) -> !; }
#[inline(never)] fn retained() { std::hint::black_box(17); }
#[inline(never)] fn terminate() -> ! { unsafe { kill(getpid(), 9); _exit(126); } }
#[inline(never)] fn run() -> ! {
    let mut step = 0;
    loop {
        match std::hint::black_box(step) {
            0 => {},
            1 => terminate(),
            _ => retained(),
        }
        step += 1;
    }
}
fn main() { run() }
