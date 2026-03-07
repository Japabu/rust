use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut};

use toyos_abi::Fd;
use toyos_abi::syscall::{self, SyscallError};

fn to_io_error(e: SyscallError) -> io::Error {
    let kind = match e {
        SyscallError::NotFound => io::ErrorKind::NotFound,
        SyscallError::PermissionDenied => io::ErrorKind::PermissionDenied,
        SyscallError::WouldBlock => io::ErrorKind::WouldBlock,
        _ => io::ErrorKind::Other,
    };
    io::Error::from(kind)
}

#[derive(Debug)]
pub struct Pipe {
    fd: Fd,
}

pub fn pipe() -> io::Result<(Pipe, Pipe)> {
    let fds = syscall::pipe();
    Ok((Pipe { fd: fds.read }, Pipe { fd: fds.write }))
}

impl Pipe {
    pub fn raw_fd(&self) -> i32 {
        self.fd.0
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        let new_fd = syscall::dup(self.fd).map_err(to_io_error)?;
        Ok(Pipe { fd: new_fd })
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        syscall::read(self.fd, buf).map_err(to_io_error)
    }

    pub fn read_buf(&self, mut buf: BorrowedCursor<'_>) -> io::Result<()> {
        let spare = buf.ensure_init().init_mut();
        let n = self.read(spare)?;
        buf.advance(n);
        Ok(())
    }

    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        match bufs.first_mut() {
            Some(b) => self.read(b),
            None => Ok(0),
        }
    }

    pub fn is_read_vectored(&self) -> bool {
        false
    }

    pub fn read_to_end(&self, buf: &mut Vec<u8>) -> io::Result<usize> {
        let mut total = 0;
        let mut tmp = [0u8; 4096];
        loop {
            let n = self.read(&mut tmp)?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&tmp[..n]);
            total += n;
        }
        Ok(total)
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        syscall::write(self.fd, buf).map_err(to_io_error)
    }

    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        match bufs.first() {
            Some(b) => self.write(b),
            None => Ok(0),
        }
    }

    pub fn is_write_vectored(&self) -> bool {
        false
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        syscall::close(self.fd);
    }
}

