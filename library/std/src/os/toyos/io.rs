use crate::io;
use crate::os::fd as fd;
use crate::sys::{AsInner, FromInner};
use toyos_abi::syscall::{self, SyscallError};

// Re-export standard fd traits
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub use fd::*;

/// Re-export the Fd newtype.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub use toyos_abi::Fd;

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl AsRawFd for crate::fs::File {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        self.as_inner().as_raw_fd()
    }
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl FromRawFd for crate::fs::File {
    #[inline]
    unsafe fn from_raw_fd(fd: RawFd) -> crate::fs::File {
        crate::fs::File::from_inner(crate::sys::fs::File::from_fd(toyos_abi::Fd(fd)))
    }
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl IntoRawFd for crate::fs::File {
    #[inline]
    fn into_raw_fd(self) -> RawFd {
        self.as_inner().raw_fd()
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

/// Non-blocking read from a file descriptor.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn read_nonblock(fd: Fd, buf: &mut [u8]) -> io::Result<usize> {
    syscall::read_nonblock(fd, buf).map_err(to_io_error)
}

/// Non-blocking write to a file descriptor.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn write_nonblock(fd: Fd, buf: &[u8]) -> io::Result<usize> {
    syscall::write_nonblock(fd, buf).map_err(to_io_error)
}

/// Close a file descriptor.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn close(fd: Fd) {
    syscall::close(fd);
}
