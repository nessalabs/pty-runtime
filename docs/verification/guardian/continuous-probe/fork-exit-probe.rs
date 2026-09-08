unsafe extern "C" { fn fork() -> i32; fn _exit(status: i32) -> !; fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32; }
#[inline(never)] fn child_only() { std::hint::black_box(17); }
#[inline(never)] fn parent_only() { std::hint::black_box(29); }
fn main() { unsafe { let pid=fork(); assert!(pid>=0); if pid==0 { child_only(); _exit(0); } parent_only(); let mut status=0; assert_eq!(waitpid(pid,&mut status,0),pid); assert_eq!(status,0); } }
