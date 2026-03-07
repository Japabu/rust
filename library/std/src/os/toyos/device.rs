//! Hardware device access.

use crate::fs::File;
use crate::io;
use crate::sys::FromInner;
use toyos_abi::Fd;
use toyos_abi::syscall::{self, DeviceType, SyscallError};

fn fd_to_file(fd: Fd) -> File {
    File::from_inner(crate::sys::fs::File::from_fd(fd))
}

fn to_io_error(e: SyscallError) -> io::Error {
    let kind = match e {
        SyscallError::NotFound => io::ErrorKind::NotFound,
        SyscallError::PermissionDenied => io::ErrorKind::PermissionDenied,
        SyscallError::AlreadyExists => io::ErrorKind::AlreadyExists,
        _ => io::ErrorKind::Other,
    };
    io::Error::from(kind)
}

/// Claim exclusive access to the keyboard device.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn open_keyboard() -> io::Result<File> {
    let fd = syscall::open_device(DeviceType::Keyboard).map_err(to_io_error)?;
    Ok(fd_to_file(fd))
}

/// Claim exclusive access to the mouse device.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn open_mouse() -> io::Result<File> {
    let fd = syscall::open_device(DeviceType::Mouse).map_err(to_io_error)?;
    Ok(fd_to_file(fd))
}

/// Claim exclusive access to the framebuffer device.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn open_framebuffer() -> io::Result<File> {
    let fd = syscall::open_device(DeviceType::Framebuffer).map_err(to_io_error)?;
    Ok(fd_to_file(fd))
}

/// Claim exclusive access to the network interface.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn open_nic() -> io::Result<File> {
    let fd = syscall::open_device(DeviceType::Nic).map_err(to_io_error)?;
    Ok(fd_to_file(fd))
}
