use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut};

#[derive(Debug)]
pub struct Pipe {
    fd: u64,
}

pub fn pipe() -> io::Result<(Pipe, Pipe)> {
    let result = toyos_abi::syscall::pipe();
    if result == u64::MAX {
        return Err(io::Error::new(io::ErrorKind::Other, "failed to create pipe"));
    }
    let read_fd = result >> 32;
    let write_fd = result & 0xFFFF_FFFF;
    Ok((Pipe { fd: read_fd }, Pipe { fd: write_fd }))
}

impl Pipe {
    pub fn raw_fd(&self) -> u64 {
        self.fd
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        panic!("Pipe::try_clone not supported on ToyOS");
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let n = toyos_abi::syscall::read(self.fd, buf.as_mut_ptr(), buf.len());
        Ok(n as usize)
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
        let n = toyos_abi::syscall::write(self.fd, buf.as_ptr(), buf.len());
        Ok(n as usize)
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
        toyos_abi::syscall::close(self.fd);
    }
}

