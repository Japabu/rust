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
