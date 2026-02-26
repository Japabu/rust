pub mod os;

#[expect(dead_code)]
#[path = "../unsupported/common.rs"]
mod unsupported_common;

pub use unsupported_common::{cleanup, init};

use core::sync::atomic::{AtomicUsize, Ordering};
use toyos_abi::syscall::*;

// argc/argv stored by _start for std::env::args()
pub(crate) static ARGC: AtomicUsize = AtomicUsize::new(0);
pub(crate) static ARGV: AtomicUsize = AtomicUsize::new(0); // *const *const u8 as usize

#[unsafe(no_mangle)]
#[unsafe(naked)]
unsafe extern "C" fn _start() -> ! {
    // Stack layout at entry (set up by kernel):
    //   [RSP]   = argc
    //   [RSP+8] = argv[0], argv[1], ..., NULL
    core::arch::naked_asm!(
        "mov rdi, [rsp]",
        "lea rsi, [rsp + 8]",
        "call {start_rust}",
        "ud2",
        start_rust = sym start_rust,
    );
}

extern "C" fn start_rust(argc: usize, argv: *const *const u8) -> ! {
    unsafe extern "C" {
        fn main() -> i32;
    }
    ARGC.store(argc, Ordering::Relaxed);
    ARGV.store(argv as usize, Ordering::Relaxed);

    // Initialize environment variables and seed defaults
    crate::sys::env::init();
    unsafe {
        crate::sys::env::setenv("HOME".as_ref(), "/".as_ref()).ok();
        crate::sys::env::setenv("XDG_CONFIG_HOME".as_ref(), "/nvme/config".as_ref()).ok();
    }

    let code = unsafe { main() };
    exit(code)
}

pub fn abort_internal() -> ! {
    exit(128 + 6); // SIGABRT-like
}

// --- I/O (unified: fd, buf, len) ---

#[inline(always)]
pub fn write(fd: u64, buf: *const u8, len: usize) -> u64 {
    syscall(SYS_WRITE, fd, buf as u64, len as u64, 0)
}

#[inline(always)]
pub fn read(fd: u64, buf: *mut u8, len: usize) -> u64 {
    syscall(SYS_READ, fd, buf as u64, len as u64, 0)
}

// --- alloc ---

#[inline(always)]
pub fn alloc(size: usize, align: usize) -> *mut u8 {
    core::ptr::with_exposed_provenance_mut(syscall(SYS_ALLOC, size as u64, align as u64, 0, 0) as usize)
}

#[inline(always)]
pub fn free(ptr: *mut u8, size: usize, align: usize) {
    syscall(SYS_FREE, ptr as u64, size as u64, align as u64, 0);
}

#[inline(always)]
pub fn realloc(ptr: *mut u8, size: usize, align: usize, new_size: usize) -> *mut u8 {
    core::ptr::with_exposed_provenance_mut(syscall(SYS_REALLOC, ptr as u64, size as u64, align as u64, new_size as u64) as usize)
}

// --- process ---

#[inline(always)]
pub fn exit(code: i32) -> ! {
    loop { syscall(SYS_EXIT, code as u64, 0, 0, 0); }
}

// --- misc ---

#[inline(always)]
pub fn random(buf: *mut u8, len: usize) {
    syscall(SYS_RANDOM, buf as u64, len as u64, 0, 0);
}

#[inline(always)]
pub fn clock() -> u64 {
    syscall(SYS_CLOCK, 0, 0, 0, 0)
}

// --- fs ---

#[inline(always)]
pub fn open(path: *const u8, path_len: usize, flags: u64) -> u64 {
    syscall(SYS_OPEN, path as u64, path_len as u64, flags, 0)
}

#[inline(always)]
pub fn close(fd: u64) -> u64 {
    syscall(SYS_CLOSE, fd, 0, 0, 0)
}

#[inline(always)]
pub fn seek(fd: u64, offset: i64, whence: u64) -> u64 {
    syscall(SYS_SEEK, fd, offset as u64, whence, 0)
}

#[inline(always)]
pub fn fstat(fd: u64) -> u64 {
    syscall(SYS_FSTAT, fd, 0, 0, 0)
}

#[inline(always)]
pub fn fsync(fd: u64) -> u64 {
    syscall(SYS_FSYNC, fd, 0, 0, 0)
}

// --- readdir / delete ---

#[inline(always)]
pub fn readdir(path: *const u8, path_len: usize, buf: *mut u8, buf_len: usize) -> u64 {
    syscall(SYS_READDIR, path as u64, path_len as u64, buf as u64, buf_len as u64)
}

#[inline(always)]
pub fn delete(path: *const u8, path_len: usize) -> u64 {
    syscall(SYS_DELETE, path as u64, path_len as u64, 0, 0)
}

// --- cwd ---

#[inline(always)]
pub fn chdir(path: *const u8, path_len: usize) -> u64 {
    syscall(SYS_CHDIR, path as u64, path_len as u64, 0, 0)
}

#[inline(always)]
pub fn getcwd(buf: *mut u8, buf_len: usize) -> u64 {
    syscall(SYS_GETCWD, buf as u64, buf_len as u64, 0, 0)
}

// --- process management ---

#[inline(always)]
pub fn pipe() -> u64 {
    syscall(SYS_PIPE, 0, 0, 0, 0)
}

#[inline(always)]
pub fn spawn(argv: *const u8, len: usize, fd_map: *const [u32; 2], fd_map_count: usize) -> u64 {
    syscall(SYS_SPAWN, argv as u64, len as u64, fd_map as u64, fd_map_count as u64)
}

#[inline(always)]
pub fn waitpid(pid: u64) -> u64 {
    syscall(SYS_WAITPID, pid, 0, 0, 0)
}

#[inline(always)]
pub fn mark_tty(fd: u64) -> u64 {
    syscall(SYS_MARK_TTY, fd, 0, 0, 0)
}

#[inline(always)]
pub fn send_msg(target_pid: u64, msg_ptr: u64) -> u64 {
    syscall(SYS_SEND_MSG, target_pid, msg_ptr, 0, 0)
}

#[inline(always)]
pub fn recv_msg(msg_ptr: u64) -> u64 {
    syscall(SYS_RECV_MSG, msg_ptr, 0, 0, 0)
}

// --- threads ---

#[inline(always)]
pub fn thread_spawn(entry: u64, stack: u64, arg: u64) -> u64 {
    syscall(SYS_THREAD_SPAWN, entry, stack, arg, 0)
}

#[inline(always)]
pub fn thread_join(tid: u64) -> u64 {
    syscall(SYS_THREAD_JOIN, tid, 0, 0, 0)
}


