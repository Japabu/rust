use std::fs::{File, OpenOptions};
use std::path::Path;
use std::io;

#[derive(Debug)]
pub struct Lock {
    _file: File,
}

impl Lock {
    pub fn new(p: &Path, wait: bool, create: bool, exclusive: bool) -> io::Result<Lock> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(create)
            .open(p)?;

        // ToyOS file locking is a no-op (single-process compilation),
        // but we go through the std API for correctness.
        if exclusive {
            if wait {
                file.lock()?;
            } else {
                file.try_lock().map_err(|e| match e {
                    std::fs::TryLockError::WouldBlock => {
                        io::Error::from(io::ErrorKind::WouldBlock)
                    }
                    std::fs::TryLockError::Error(err) => err,
                })?;
            }
        } else if wait {
            file.lock_shared()?;
        } else {
            file.try_lock_shared().map_err(|e| match e {
                std::fs::TryLockError::WouldBlock => {
                    io::Error::from(io::ErrorKind::WouldBlock)
                }
                std::fs::TryLockError::Error(err) => err,
            })?;
        }

        Ok(Lock { _file: file })
    }

    pub fn error_unsupported(_err: &io::Error) -> bool {
        false
    }
}
