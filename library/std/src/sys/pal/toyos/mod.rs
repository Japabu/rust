pub mod os;

#[expect(dead_code)]
#[path = "../unsupported/common.rs"]
mod unsupported_common;

pub use unsupported_common::{cleanup, init, unsupported};

use core::sync::atomic::{AtomicUsize, Ordering};

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
    unsafe { crate::sys::env::setenv("HOME".as_ref(), "/".as_ref()).ok(); }

    let code = unsafe { main() };
    exit(code)
}

pub fn abort_internal() -> ! {
    exit(128 + 6); // SIGABRT-like
}

// Syscall numbers (must match kernel)
const SYS_WRITE: u64 = 0;
const SYS_READ: u64 = 1;
const SYS_ALLOC: u64 = 2;
const SYS_FREE: u64 = 3;
const SYS_REALLOC: u64 = 4;
const SYS_EXIT: u64 = 5;
const SYS_RANDOM: u64 = 6;
#[allow(dead_code)]
const SYS_SCREEN_SIZE: u64 = 7;
const SYS_CLOCK: u64 = 8;
const SYS_OPEN: u64 = 9;
const SYS_CLOSE: u64 = 10;
const SYS_READ_FILE: u64 = 11;
const SYS_WRITE_FILE: u64 = 12;
const SYS_SEEK: u64 = 13;
const SYS_FSTAT: u64 = 14;
const SYS_FSYNC: u64 = 15;
const SYS_EXEC: u64 = 16;

#[inline(always)]
fn syscall(num: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> u64 {
    let ret: u64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rdi") num,
            in("rsi") a1,
            in("rdx") a2,
            in("r8") a3,
            in("r9") a4,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
        );
    }
    ret
}

// --- stdio ---

#[inline(always)]
pub fn write(buf: *const u8, len: usize) -> isize {
    syscall(SYS_WRITE, buf as u64, len as u64, 0, 0) as isize
}

#[inline(always)]
pub fn read(buf: *mut u8, len: usize) -> isize {
    syscall(SYS_READ, buf as u64, len as u64, 0, 0) as isize
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

#[inline(always)]
pub fn exec(path: *const u8, path_len: usize, out_buf: *mut u8, out_buf_len: usize) -> u64 {
    syscall(SYS_EXEC, path as u64, path_len as u64, out_buf as u64, out_buf_len as u64)
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

#[inline(always)]
#[allow(dead_code)]
pub fn screen_size() -> u64 {
    syscall(SYS_SCREEN_SIZE, 0, 0, 0, 0)
}

// --- fs ---

#[inline(always)]
pub fn open(path: *const u8, path_len: usize, flags: u64) -> u64 {
    syscall(SYS_OPEN, path as u64, path_len as u64, flags, 0)
}

#[inline(always)]
pub fn close(fd: u64) {
    syscall(SYS_CLOSE, fd, 0, 0, 0);
}

#[inline(always)]
pub fn read_file(fd: u64, buf: *mut u8, len: usize) -> u64 {
    syscall(SYS_READ_FILE, fd, buf as u64, len as u64, 0)
}

#[inline(always)]
pub fn write_file(fd: u64, buf: *const u8, len: usize) -> u64 {
    syscall(SYS_WRITE_FILE, fd, buf as u64, len as u64, 0)
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
pub fn fsync(fd: u64) {
    syscall(SYS_FSYNC, fd, 0, 0, 0);
}
