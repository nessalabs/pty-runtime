unsafe extern "C" { fn fork() -> i32; fn _exit(status: i32) -> !; fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32; fn pipe(p: *mut i32) -> i32; fn write(fd: i32,p:*const u8,n:usize)->isize; fn read(fd:i32,p:*mut u8,n:usize)->isize; fn kill(pid:i32,sig:i32)->i32; fn pause()->i32; }
#[inline(never)] fn child_only() { std::hint::black_box(17); }
#[inline(never)] fn parent_only() { std::hint::black_box(29); }
#[inline(never)] fn shared() { std::hint::black_box(31); }
fn main() { unsafe { let mut fds=[0;2]; assert_eq!(pipe(fds.as_mut_ptr()),0); let pid=fork(); assert!(pid>=0); if pid==0 { for _ in 0..10000 { child_only(); shared(); } assert_eq!(write(fds[1],b"!".as_ptr(),1),1); pause(); _exit(125); } for _ in 0..10000 { parent_only(); shared(); } let mut byte=0; assert_eq!(read(fds[0],&mut byte,1),1); assert_eq!(kill(pid,9),0); let mut status=0; assert_eq!(waitpid(pid,&mut status,0),pid); assert_eq!(status&127,9); println!("ready"); pause(); } }
