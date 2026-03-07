//! Process name registry for service discovery.

use crate::io;
use toyos_abi::syscall::{self, SyscallError};

/// Register the current process under a name so other processes can find it.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn register(name: &str) -> io::Result<()> {
    syscall::register_name(name).map_err(|e| {
        let kind = match e {
            SyscallError::AlreadyExists => io::ErrorKind::AlreadyExists,
            _ => io::ErrorKind::Other,
        };
        io::Error::from(kind)
    })
}

/// Find the PID of a process registered under the given name.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn find(name: &str) -> Option<u32> {
    syscall::find_pid(name).map(|pid| pid.0)
}
