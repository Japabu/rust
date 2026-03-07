//! Extended pipe operations.

use crate::fs::File;
use crate::io;
use crate::sys::FromInner;
use toyos_abi::syscall::{self, SyscallError};

fn fd_to_file(fd: syscall::Fd) -> File {
    File::from_inner(crate::sys::fs::File::from_fd(fd))
}

fn to_io_error(e: SyscallError) -> io::Error {
    let kind = match e {
        SyscallError::NotFound => io::ErrorKind::NotFound,
        SyscallError::InvalidArgument => io::ErrorKind::InvalidInput,
        _ => io::ErrorKind::Other,
    };
    io::Error::from(kind)
}

/// Create a pipe with a specific buffer capacity.
/// Returns `(read_end, write_end)`.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn create_with_capacity(capacity: usize) -> (File, File) {
    let fds = syscall::pipe_with_capacity(capacity);
    (fd_to_file(fds.read), fd_to_file(fds.write))
}

/// Get the internal pipe ID for a file descriptor.
/// Used to share pipe access across processes via [`open_by_id`].
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn id(raw_fd: i32) -> io::Result<u64> {
    syscall::pipe_id(syscall::Fd(raw_fd)).map_err(to_io_error)
}

/// Open an existing pipe by its internal ID.
/// `read`: `true` for the read end, `false` for the write end.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn open_by_id(pipe_id: u64, read: bool) -> io::Result<File> {
    let mode = if read { 0 } else { 1 };
    let fd = syscall::pipe_open(pipe_id, mode).map_err(to_io_error)?;
    Ok(fd_to_file(fd))
}
