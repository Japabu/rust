use crate::sys::AsInner;

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

/// Result of a [`poll`] call.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub struct PollResult {
    mask: u64,
    fd_count: usize,
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl PollResult {
    /// Whether the file descriptor at `index` is ready.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn fd(&self, index: usize) -> bool {
        self.mask & (1 << index) != 0
    }

    /// Whether the process message queue has messages.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn messages(&self) -> bool {
        self.mask & (1 << self.fd_count) != 0
    }
}

/// Poll file descriptors and the message queue for readiness.
/// Blocks until at least one source has data.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn poll(fds: &[u64]) -> PollResult {
    let mask = crate::sys::stdio::poll(fds.as_ptr() as u64, fds.len() as u64);
    PollResult { mask, fd_count: fds.len() }
}

/// Read from a file descriptor. Returns bytes read.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn read_fd(fd: u64, buf: &mut [u8]) -> usize {
    crate::sys::stdio::read_fd(fd, buf.as_mut_ptr(), buf.len()) as usize
}

/// Get the raw FD number from a std type.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub trait AsRawFd {
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    fn as_raw_fd(&self) -> u64;
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl AsRawFd for crate::process::ChildStdin {
    fn as_raw_fd(&self) -> u64 {
        self.as_inner().raw_fd()
    }
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl AsRawFd for crate::process::ChildStdout {
    fn as_raw_fd(&self) -> u64 {
        self.as_inner().raw_fd()
    }
}
