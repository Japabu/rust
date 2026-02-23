use crate::sys::process as imp;
use crate::sys::FromInner;

/// Create a `Stdio` that pipes through a tty-typed file descriptor.
///
/// Like `Stdio::piped()`, but the pipe endpoints are marked as tty so the
/// child process gets canonical mode (echo + line editing) on its stdin.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn tty_piped() -> crate::process::Stdio {
    crate::process::Stdio::from_inner(imp::Stdio::MakeTtyPipe)
}
