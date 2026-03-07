use crate::io;
use crate::os::fd::{self, AsRawFd, RawFd};
use crate::sys::AsInner;
use toyos_abi::syscall::{self, SyscallError};

// Re-export standard fd traits
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub use fd::*;

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl AsRawFd for crate::fs::File {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.as_inner().as_raw_fd()
    }
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl AsRawFd for crate::net::TcpStream {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.as_inner().as_raw_fd()
    }
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl AsRawFd for crate::net::TcpListener {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.as_inner().as_raw_fd()
    }
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl AsRawFd for crate::process::ChildStdin {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.as_inner().raw_fd()
    }
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl AsRawFd for crate::process::ChildStdout {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.as_inner().raw_fd()
    }
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl AsRawFd for crate::process::ChildStderr {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.as_inner().raw_fd()
    }
}

/// Switch stdin between canonical and raw mode.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn set_stdin_raw(raw: bool) {
    crate::sys::stdio::set_stdin_raw(raw);
}

fn to_io_error(e: SyscallError) -> io::Error {
    let kind = match e {
        SyscallError::WouldBlock => io::ErrorKind::WouldBlock,
        SyscallError::InvalidArgument => io::ErrorKind::InvalidInput,
        _ => io::ErrorKind::Other,
    };
    io::Error::from(kind)
}

/// Non-blocking read from a raw file descriptor.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn read_nonblock(raw_fd: i32, buf: &mut [u8]) -> io::Result<usize> {
    syscall::read_nonblock(syscall::Fd(raw_fd), buf).map_err(to_io_error)
}

/// Non-blocking write to a raw file descriptor.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn write_nonblock(raw_fd: i32, buf: &[u8]) -> io::Result<usize> {
    syscall::write_nonblock(syscall::Fd(raw_fd), buf).map_err(to_io_error)
}

/// Close a raw file descriptor.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn close(raw_fd: i32) {
    syscall::close(syscall::Fd(raw_fd));
}
