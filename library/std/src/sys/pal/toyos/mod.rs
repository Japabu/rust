pub mod os;

#[expect(dead_code)]
#[path = "../unsupported/common.rs"]
mod unsupported_common;

pub use unsupported_common::{cleanup, init};

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

// Syscall numbers (must match kernel)
const SYS_WRITE: u64 = 0;
const SYS_READ: u64 = 1;
const SYS_ALLOC: u64 = 2;
const SYS_FREE: u64 = 3;
const SYS_REALLOC: u64 = 4;
const SYS_EXIT: u64 = 5;
const SYS_RANDOM: u64 = 6;
const SYS_SCREEN_SIZE: u64 = 7;
const SYS_CLOCK: u64 = 8;
const SYS_OPEN: u64 = 9;
const SYS_CLOSE: u64 = 10;
const SYS_SEEK: u64 = 13;
const SYS_FSTAT: u64 = 14;
const SYS_FSYNC: u64 = 15;
const SYS_READDIR: u64 = 17;
const SYS_DELETE: u64 = 18;
const SYS_SHUTDOWN: u64 = 19;
const SYS_CHDIR: u64 = 20;
const SYS_SET_KEYBOARD_LAYOUT: u64 = 23;
const SYS_GETCWD: u64 = 21;
const SYS_PIPE: u64 = 24;
const SYS_SPAWN: u64 = 25;
const SYS_WAITPID: u64 = 26;
const SYS_POLL: u64 = 27;
const SYS_MARK_TTY: u64 = 28;
const SYS_SEND_MSG: u64 = 29;
const SYS_RECV_MSG: u64 = 30;
const SYS_OPEN_DEVICE: u64 = 31;
const SYS_REGISTER_NAME: u64 = 32;
const SYS_FIND_PID: u64 = 33;
const SYS_SET_SCREEN_SIZE: u64 = 34;
const SYS_GPU_PRESENT: u64 = 35;
const SYS_ALLOC_SHARED: u64 = 36;
const SYS_GRANT_SHARED: u64 = 37;
const SYS_MAP_SHARED: u64 = 38;
const SYS_RELEASE_SHARED: u64 = 39;
const SYS_THREAD_SPAWN: u64 = 40;
const SYS_THREAD_JOIN: u64 = 41;
const SYS_CLOCK_REALTIME: u64 = 42;
const SYS_GPU_SET_CURSOR: u64 = 43;
const SYS_GPU_MOVE_CURSOR: u64 = 44;
const SYS_SYSINFO: u64 = 45;
const SYS_NET_INFO: u64 = 46;
const SYS_NET_SEND: u64 = 47;
const SYS_NET_RECV: u64 = 48;

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

// --- toyos-specific ---

#[inline(always)]
pub fn screen_size() -> u64 {
    syscall(SYS_SCREEN_SIZE, 0, 0, 0, 0)
}

#[inline(always)]
pub fn set_screen_size(width: u32, height: u32) {
    syscall(SYS_SET_SCREEN_SIZE, width as u64, height as u64, 0, 0);
}

#[inline(always)]
pub fn gpu_present(x: u64, y: u64, w: u64, h: u64) {
    syscall(SYS_GPU_PRESENT, x, y, w, h);
}

#[inline(always)]
pub fn gpu_set_cursor(hot_x: u64, hot_y: u64) {
    syscall(SYS_GPU_SET_CURSOR, hot_x, hot_y, 0, 0);
}

#[inline(always)]
pub fn gpu_move_cursor(x: u64, y: u64) {
    syscall(SYS_GPU_MOVE_CURSOR, x, y, 0, 0);
}

#[inline(always)]
pub fn shutdown() {
    syscall(SYS_SHUTDOWN, 0, 0, 0, 0);
}

#[inline(always)]
pub fn set_keyboard_layout(name: *const u8, len: usize) -> u64 {
    syscall(SYS_SET_KEYBOARD_LAYOUT, name as u64, len as u64, 0, 0)
}

// --- process management ---

#[inline(always)]
pub fn pipe() -> u64 {
    syscall(SYS_PIPE, 0, 0, 0, 0)
}

#[inline(always)]
pub fn spawn(argv: *const u8, len: usize, stdin_fd: u64, stdout_fd: u64) -> u64 {
    syscall(SYS_SPAWN, argv as u64, len as u64, stdin_fd, stdout_fd)
}

#[inline(always)]
pub fn waitpid(pid: u64) -> u64 {
    syscall(SYS_WAITPID, pid, 0, 0, 0)
}

#[inline(always)]
pub fn poll(fds_ptr: u64, fds_len: u64, timeout_nanos: u64) -> u64 {
    syscall(SYS_POLL, fds_ptr, fds_len, timeout_nanos, 0)
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

// --- devices & names ---

#[inline(always)]
pub fn open_device(device_type: u64) -> u64 {
    syscall(SYS_OPEN_DEVICE, device_type, 0, 0, 0)
}

#[inline(always)]
pub fn register_name(name: *const u8, len: usize) -> u64 {
    syscall(SYS_REGISTER_NAME, name as u64, len as u64, 0, 0)
}

#[inline(always)]
pub fn find_pid(name: *const u8, len: usize) -> u64 {
    syscall(SYS_FIND_PID, name as u64, len as u64, 0, 0)
}

// --- shared memory ---

#[inline(always)]
pub fn alloc_shared(size: u64) -> u64 {
    syscall(SYS_ALLOC_SHARED, size, 0, 0, 0)
}

#[inline(always)]
pub fn grant_shared(token: u64, target_pid: u64) -> u64 {
    syscall(SYS_GRANT_SHARED, token, target_pid, 0, 0)
}

#[inline(always)]
pub fn map_shared(token: u64) -> u64 {
    syscall(SYS_MAP_SHARED, token, 0, 0, 0)
}

#[inline(always)]
pub fn release_shared(token: u64) -> u64 {
    syscall(SYS_RELEASE_SHARED, token, 0, 0, 0)
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

/// Read wall-clock time: (hours << 16) | (minutes << 8) | seconds.
#[inline(always)]
pub fn clock_realtime() -> u64 {
    syscall(SYS_CLOCK_REALTIME, 0, 0, 0, 0)
}

#[inline(always)]
pub fn sysinfo(buf: *mut u8, len: usize) -> u64 {
    syscall(SYS_SYSINFO, buf as u64, len as u64, 0, 0)
}

// --- networking ---

#[inline(always)]
pub fn net_info(buf: *mut u8, len: usize) -> u64 {
    syscall(SYS_NET_INFO, buf as u64, len as u64, 0, 0)
}

#[inline(always)]
pub fn net_send(buf: *const u8, len: usize) -> u64 {
    syscall(SYS_NET_SEND, buf as u64, len as u64, 0, 0)
}

#[inline(always)]
pub fn net_recv(buf: *mut u8, len: usize, timeout_nanos: u64) -> u64 {
    syscall(SYS_NET_RECV, buf as u64, len as u64, timeout_nanos, 0)
}

