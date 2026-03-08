use crate::sys::process as imp;
use crate::sys::{AsInnerMut, FromInner};

/// Create a `Stdio` that pipes through a tty-typed file descriptor.
///
/// Like `Stdio::piped()`, but the pipe endpoints are marked as tty so the
/// child process gets canonical mode (echo + line editing) on its stdin.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn tty_piped() -> crate::process::Stdio {
    crate::process::Stdio::from_inner(imp::Stdio::MakeTtyPipe)
}

/// ToyOS-specific extensions to [`process::Command`].
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub trait CommandExt {
    /// Pass an additional file descriptor to the child process.
    ///
    /// The child process will inherit `parent_fd` as `child_fd`.
    /// This is useful for passing pipe file descriptors (e.g., for jobserver
    /// protocols) to child processes.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    fn inherit_fd(&mut self, child_fd: u32, parent_fd: u32) -> &mut Self;
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl CommandExt for crate::process::Command {
    fn inherit_fd(&mut self, child_fd: u32, parent_fd: u32) -> &mut Self {
        self.as_inner_mut().inherit_fd(child_fd, parent_fd);
        self
    }
}
