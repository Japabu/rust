use crate::ffi::{OsStr, OsString};
use crate::marker::PhantomData;
use crate::path::{self, PathBuf};
use crate::{fmt, io};

pub fn getcwd() -> io::Result<PathBuf> {
    let mut buf = [0u8; 256];
    let n = super::getcwd(buf.as_mut_ptr(), buf.len());
    if n == u64::MAX {
        return Err(io::Error::new(io::ErrorKind::Other, "getcwd failed"));
    }
    let s = core::str::from_utf8(&buf[..n as usize])
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "invalid utf-8 in cwd"))?;
    Ok(PathBuf::from(s))
}

pub fn chdir(p: &path::Path) -> io::Result<()> {
    let bytes = p.as_os_str().as_encoded_bytes();
    let result = super::chdir(bytes.as_ptr(), bytes.len());
    if result == u64::MAX {
        Err(io::Error::new(io::ErrorKind::NotFound, "no such directory"))
    } else {
        Ok(())
    }
}

pub struct SplitPaths<'a>(!, PhantomData<&'a ()>);

pub fn split_paths(_unparsed: &OsStr) -> SplitPaths<'_> {
    panic!("unsupported")
}

impl<'a> Iterator for SplitPaths<'a> {
    type Item = PathBuf;
    fn next(&mut self) -> Option<PathBuf> {
        self.0
    }
}

#[derive(Debug)]
pub struct JoinPathsError;

pub fn join_paths<I, T>(_paths: I) -> Result<OsString, JoinPathsError>
where
    I: Iterator<Item = T>,
    T: AsRef<OsStr>,
{
    Err(JoinPathsError)
}

impl fmt::Display for JoinPathsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        "not supported on this platform yet".fmt(f)
    }
}

impl crate::error::Error for JoinPathsError {}

pub fn current_exe() -> io::Result<PathBuf> {
    panic!("current_exe not implemented")
}

pub fn temp_dir() -> PathBuf {
    panic!("no filesystem on this platform")
}

pub fn home_dir() -> Option<PathBuf> {
    None
}

pub fn exit(code: i32) -> ! {
    super::exit(code)
}

pub fn getpid() -> u32 {
    panic!("no pids on this platform")
}
