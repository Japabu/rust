//! Unified polling of file descriptors and the process message queue.

use crate::time::Duration;
use toyos_abi::syscall;

/// Interest flag: watch for readability.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub const READABLE: u64 = syscall::POLL_READABLE;

/// Interest flag: watch for writability.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub const WRITABLE: u64 = syscall::POLL_WRITABLE;

/// Mask to extract the fd number from a poll entry (strips interest flags).
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub const FD_MASK: u64 = syscall::POLL_FD_MASK;

/// Result of a poll operation.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub struct PollResult {
    inner: syscall::PollResult,
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl PollResult {
    /// Whether the file descriptor at `index` is ready.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn fd_ready(&self, index: usize) -> bool {
        self.inner.fd(index)
    }

    /// Whether the process message queue has messages.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn has_messages(&self) -> bool {
        self.inner.messages()
    }
}

/// Poll file descriptors and the message queue.
///
/// Each entry in `fds` is a raw fd number, optionally OR'd with
/// [`READABLE`] or [`WRITABLE`] interest flags.
///
/// `timeout`: `None` blocks forever, `Some(duration)` times out.
/// Returns immediately if any source is ready.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn poll(fds: &[u64], timeout: Option<Duration>) -> PollResult {
    let nanos = timeout.map(|d| d.as_nanos() as u64);
    PollResult { inner: syscall::poll_timeout(fds, nanos) }
}
