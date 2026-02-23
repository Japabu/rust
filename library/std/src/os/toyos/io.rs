/// Switch stdin between canonical (line-buffered with echo) and raw mode.
///
/// In canonical mode (the default), `Stdin::read()` echoes characters, handles
/// backspace, and buffers a complete line before returning.
///
/// In raw mode, `Stdin::read()` passes bytes through directly from the kernel.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn set_stdin_raw(raw: bool) {
    crate::sys::stdio::set_stdin_raw(raw);
}

/// Read from stdin directly, bypassing `BufReader` buffering.
///
/// Use this for raw byte-at-a-time input (e.g. in a readline loop with
/// `set_stdin_raw(true)`).
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn read_stdin_raw(buf: &mut [u8]) -> crate::io::Result<usize> {
    crate::sys::stdio::read_stdin_raw(buf)
}

/// Get the screen size as (rows, columns).
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn screen_size() -> (usize, usize) {
    crate::sys::stdio::screen_size()
}

/// Set the active keyboard layout by name. Returns `true` on success.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn set_keyboard_layout(name: &str) -> bool {
    crate::sys::stdio::set_keyboard_layout(name)
}

/// Shut down the machine. Does not return.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn shutdown() -> ! {
    crate::sys::stdio::shutdown()
}

/// Poll two file descriptors for readiness. Blocks until at least one has data.
/// Returns a bitmask: bit 0 = fd1 ready, bit 1 = fd2 ready.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn poll(fd1: u64, fd2: u64) -> u64 {
    crate::sys::stdio::poll(fd1, fd2)
}

/// Create a pipe. Returns (read_fd, write_fd).
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn pipe() -> (u64, u64) {
    let result = crate::sys::stdio::pipe();
    (result >> 32, result & 0xFFFF_FFFF)
}

/// Spawn a child process with the given arguments and FD assignments.
/// Returns the child PID.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn spawn(args: &[&str], stdin_fd: u64, stdout_fd: u64) -> u64 {
    let mut buf = crate::vec::Vec::new();
    for arg in args {
        buf.extend_from_slice(arg.as_bytes());
        buf.push(0);
    }
    crate::sys::stdio::spawn(buf.as_ptr(), buf.len(), stdin_fd, stdout_fd)
}

/// Wait for a child process to exit. Returns the exit code.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn waitpid(pid: u64) -> i32 {
    crate::sys::stdio::waitpid(pid) as i32
}

/// Read from a file descriptor. Returns bytes read.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn read_fd(fd: u64, buf: &mut [u8]) -> usize {
    crate::sys::stdio::read_fd(fd, buf.as_mut_ptr(), buf.len()) as usize
}

/// Write to a file descriptor. Returns bytes written.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn write_fd(fd: u64, buf: &[u8]) -> usize {
    crate::sys::stdio::write_fd(fd, buf.as_ptr(), buf.len()) as usize
}

/// Close a file descriptor.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn close_fd(fd: u64) {
    crate::sys::stdio::close_fd(fd);
}
