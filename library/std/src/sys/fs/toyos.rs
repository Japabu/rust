use crate::ffi::OsString;
use crate::fmt;
use crate::fs::TryLockError;
use crate::hash::Hash;
use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut, SeekFrom};
use crate::path::{Path, PathBuf};
pub use crate::sys::fs::common::Dir;
use crate::sys::time::SystemTime;

use toyos_abi::syscall::{self, Fd, OpenFlags, SyscallError};

fn to_io_error(e: SyscallError) -> io::Error {
    let kind = match e {
        SyscallError::NotFound => io::ErrorKind::NotFound,
        SyscallError::PermissionDenied => io::ErrorKind::PermissionDenied,
        SyscallError::AlreadyExists => io::ErrorKind::AlreadyExists,
        SyscallError::InvalidArgument => io::ErrorKind::InvalidInput,
        SyscallError::WouldBlock => io::ErrorKind::WouldBlock,
        _ => io::ErrorKind::Other,
    };
    io::Error::from(kind)
}

pub struct File(Fd);

#[derive(Clone)]
pub struct FileAttr {
    size: u64,
    file_type: syscall::FileType,
    mtime: u64,
}

pub struct ReadDir {
    entries: Vec<DirEntry>,
    index: usize,
}

pub struct DirEntry {
    dir_path: PathBuf,
    name: OsString,
    size: u64,
    is_dir: bool,
}

#[derive(Clone, Debug)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
    create_new: bool,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct FileTimes {}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FilePermissions {
    readonly: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct FileType {
    is_file: bool,
    is_dir: bool,
}

#[derive(Debug)]
pub struct DirBuilder {}

impl FileAttr {
    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn perm(&self) -> FilePermissions {
        FilePermissions { readonly: false }
    }

    pub fn file_type(&self) -> FileType {
        FileType {
            is_file: self.file_type == syscall::FileType::File,
            is_dir: self.file_type == syscall::FileType::Pipe, // directories use readdir path, not fstat
        }
    }

    pub fn modified(&self) -> io::Result<SystemTime> {
        Ok(SystemTime::from_nanos(self.mtime))
    }

    pub fn accessed(&self) -> io::Result<SystemTime> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "ToyOS does not track access time"))
    }

    pub fn created(&self) -> io::Result<SystemTime> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "ToyOS does not track creation time"))
    }
}

impl FilePermissions {
    pub fn readonly(&self) -> bool {
        self.readonly
    }

    pub fn set_readonly(&mut self, readonly: bool) {
        self.readonly = readonly;
    }
}

impl FileTimes {
    pub fn set_accessed(&mut self, _t: SystemTime) {}
    pub fn set_modified(&mut self, _t: SystemTime) {}
}

impl FileType {
    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    pub fn is_file(&self) -> bool {
        self.is_file
    }

    pub fn is_symlink(&self) -> bool {
        false
    }
}

impl fmt::Debug for ReadDir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadDir").finish_non_exhaustive()
    }
}

impl Iterator for ReadDir {
    type Item = io::Result<DirEntry>;

    fn next(&mut self) -> Option<io::Result<DirEntry>> {
        if self.index < self.entries.len() {
            let i = self.index;
            self.index += 1;
            let e = &self.entries[i];
            Some(Ok(DirEntry {
                dir_path: e.dir_path.clone(),
                name: e.name.clone(),
                size: e.size,
                is_dir: e.is_dir,
            }))
        } else {
            None
        }
    }
}

impl DirEntry {
    pub fn path(&self) -> PathBuf {
        self.dir_path.join(&self.name)
    }

    pub fn file_name(&self) -> OsString {
        self.name.clone()
    }

    pub fn metadata(&self) -> io::Result<FileAttr> {
        Ok(FileAttr {
            size: self.size,
            file_type: if self.is_dir { syscall::FileType::Pipe } else { syscall::FileType::File },
            mtime: 0,
        })
    }

    pub fn file_type(&self) -> io::Result<FileType> {
        Ok(FileType {
            is_file: !self.is_dir,
            is_dir: self.is_dir,
        })
    }
}

impl OpenOptions {
    pub fn new() -> OpenOptions {
        OpenOptions {
            read: false,
            write: false,
            append: false,
            truncate: false,
            create: false,
            create_new: false,
        }
    }

    pub fn read(&mut self, read: bool) { self.read = read; }
    pub fn write(&mut self, write: bool) { self.write = write; }
    pub fn append(&mut self, append: bool) { self.append = append; }
    pub fn truncate(&mut self, truncate: bool) { self.truncate = truncate; }
    pub fn create(&mut self, create: bool) { self.create = create; }
    pub fn create_new(&mut self, create_new: bool) { self.create_new = create_new; }

    fn to_flags(&self) -> OpenFlags {
        let mut flags = OpenFlags(0);
        if self.read { flags |= OpenFlags::READ; }
        if self.write || self.append { flags |= OpenFlags::WRITE; }
        if self.create || self.create_new { flags |= OpenFlags::CREATE; }
        if self.truncate { flags |= OpenFlags::TRUNCATE; }
        flags
    }
}

impl File {
    pub fn raw_fd(&self) -> u64 {
        self.0.0
    }

    pub fn open(path: &Path, opts: &OpenOptions) -> io::Result<File> {
        let path_bytes = path.as_os_str().as_encoded_bytes();
        let fd = syscall::open(path_bytes, opts.to_flags()).map_err(to_io_error)?;
        Ok(File(fd))
    }

    pub fn file_attr(&self) -> io::Result<FileAttr> {
        let stat = syscall::fstat(self.0).map_err(to_io_error)?;
        Ok(FileAttr { size: stat.size, file_type: stat.file_type, mtime: stat.mtime })
    }

    pub fn fsync(&self) -> io::Result<()> {
        syscall::fsync(self.0).map_err(to_io_error)
    }

    pub fn datasync(&self) -> io::Result<()> {
        self.fsync()
    }

    pub fn lock(&self) -> io::Result<()> { Ok(()) }
    pub fn lock_shared(&self) -> io::Result<()> { Ok(()) }
    pub fn try_lock(&self) -> Result<(), TryLockError> { Ok(()) }
    pub fn try_lock_shared(&self) -> Result<(), TryLockError> { Ok(()) }
    pub fn unlock(&self) -> io::Result<()> { Ok(()) }

    pub fn truncate(&self, size: u64) -> io::Result<()> {
        syscall::ftruncate(self.0, size).map_err(to_io_error)
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        syscall::read(self.0, buf).map_err(to_io_error)
    }

    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        let mut total = 0;
        for buf in bufs {
            match self.read(buf) {
                Ok(0) => break,
                Ok(n) => total += n,
                Err(e) => if total == 0 { return Err(e) } else { break },
            }
        }
        Ok(total)
    }

    pub fn is_read_vectored(&self) -> bool { false }

    pub fn read_buf(&self, mut cursor: BorrowedCursor<'_>) -> io::Result<()> {
        let n = self.read(cursor.ensure_init().init_mut())?;
        cursor.advance(n);
        Ok(())
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        syscall::write(self.0, buf).map_err(to_io_error)
    }

    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        let mut total = 0;
        for buf in bufs {
            match self.write(buf) {
                Ok(0) => break,
                Ok(n) => total += n,
                Err(e) => if total == 0 { return Err(e) } else { break },
            }
        }
        Ok(total)
    }

    pub fn is_write_vectored(&self) -> bool { false }

    pub fn flush(&self) -> io::Result<()> {
        self.fsync()
    }

    pub fn seek(&self, pos: SeekFrom) -> io::Result<u64> {
        let abi_pos = match pos {
            SeekFrom::Start(n) => syscall::SeekFrom::Start(n),
            SeekFrom::Current(n) => syscall::SeekFrom::Current(n),
            SeekFrom::End(n) => syscall::SeekFrom::End(n),
        };
        syscall::seek(self.0, abi_pos).map_err(to_io_error)
    }

    pub fn size(&self) -> Option<io::Result<u64>> {
        Some(self.file_attr().map(|a| a.size))
    }

    pub fn tell(&self) -> io::Result<u64> {
        self.seek(SeekFrom::Current(0))
    }

    pub fn duplicate(&self) -> io::Result<File> {
        let new_fd = syscall::dup(self.0).map_err(to_io_error)?;
        Ok(File(new_fd))
    }

    pub fn set_permissions(&self, _perm: FilePermissions) -> io::Result<()> {
        Ok(())
    }

    pub fn set_times(&self, _times: FileTimes) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for File {
    fn drop(&mut self) {
        syscall::close(self.0);
    }
}

impl DirBuilder {
    pub fn new() -> DirBuilder {
        DirBuilder {}
    }

    pub fn mkdir(&self, p: &Path) -> io::Result<()> {
        let path_bytes = p.as_os_str().as_encoded_bytes();
        syscall::mkdir(path_bytes).map_err(to_io_error)
    }
}

impl fmt::Debug for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "File({})", self.0.0)
    }
}

pub fn readdir(p: &Path) -> io::Result<ReadDir> {
    let path_bytes = p.as_os_str().as_encoded_bytes();
    let mut buf = vec![0u8; 65536];
    let n = syscall::readdir(path_bytes, &mut buf);
    if n == 0 {
        // Could be empty dir or not found — try to distinguish
        // For now treat 0 as empty dir
    }

    let data = &buf[..n];
    let mut entries = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        if pos + 1 >= data.len() { break; }
        let entry_type = data[pos];
        pos += 1;
        let name_end = match data[pos..].iter().position(|&b| b == 0) {
            Some(i) => pos + i,
            None => break,
        };
        let name = core::str::from_utf8(&data[pos..name_end]).unwrap_or("");
        pos = name_end + 1;
        if pos + 8 > data.len() { break; }
        let size = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        entries.push(DirEntry {
            dir_path: p.to_path_buf(),
            name: OsString::from(name),
            size,
            is_dir: entry_type == 2,
        });
    }

    Ok(ReadDir { entries, index: 0 })
}

pub fn unlink(p: &Path) -> io::Result<()> {
    let path_bytes = p.as_os_str().as_encoded_bytes();
    syscall::delete(path_bytes).map_err(to_io_error)
}

pub fn rename(old: &Path, new: &Path) -> io::Result<()> {
    let old_bytes = old.as_os_str().as_encoded_bytes();
    let new_bytes = new.as_os_str().as_encoded_bytes();
    syscall::rename(old_bytes, new_bytes).map_err(to_io_error)
}

pub fn set_perm(_p: &Path, _perm: FilePermissions) -> io::Result<()> {
    Ok(())
}

pub fn rmdir(p: &Path) -> io::Result<()> {
    let path_bytes = p.as_os_str().as_encoded_bytes();
    syscall::rmdir(path_bytes).map_err(to_io_error)
}

pub fn remove_dir_all(path: &Path) -> io::Result<()> {
    for entry in readdir(path)? {
        let entry = entry?;
        let child_path = entry.path();
        if entry.file_type()?.is_dir() {
            remove_dir_all(&child_path)?;
        } else {
            unlink(&child_path)?;
        }
    }
    Ok(())
}

pub fn exists(path: &Path) -> io::Result<bool> {
    let path_bytes = path.as_os_str().as_encoded_bytes();
    if let Ok(fd) = syscall::open(path_bytes, OpenFlags::READ) {
        syscall::close(fd);
        return Ok(true);
    }
    // open() only works for files — check if it's a directory via readdir
    let mut buf = [0u8; 1];
    let n = syscall::readdir(path_bytes, &mut buf);
    Ok(n > 0 || !path_bytes.is_empty())
}

pub fn readlink(_p: &Path) -> io::Result<PathBuf> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "no symlinks on ToyOS"))
}

pub fn symlink(_original: &Path, _link: &Path) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "no symlinks on ToyOS"))
}

pub fn link(_src: &Path, _dst: &Path) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "no hard links on ToyOS"))
}

pub fn stat(path: &Path) -> io::Result<FileAttr> {
    let path_bytes = path.as_os_str().as_encoded_bytes();
    if let Ok(fd) = syscall::open(path_bytes, OpenFlags::READ) {
        let result = syscall::fstat(fd);
        syscall::close(fd);
        let st = result.map_err(to_io_error)?;
        return Ok(FileAttr { size: st.size, file_type: st.file_type, mtime: st.mtime });
    }
    // open() only works for files — check if it's a directory via readdir
    let mut buf = [0u8; 1];
    let n = syscall::readdir(path_bytes, &mut buf);
    if n > 0 {
        return Ok(FileAttr { size: 0, file_type: syscall::FileType::Pipe, mtime: 0 }); // directory
    }
    Err(io::Error::new(io::ErrorKind::NotFound, "file not found"))
}

pub fn lstat(p: &Path) -> io::Result<FileAttr> {
    stat(p)
}

pub fn canonicalize(p: &Path) -> io::Result<PathBuf> {
    crate::path::absolute(p)
}

pub fn copy(from: &Path, to: &Path) -> io::Result<u64> {
    let reader = File::open(from, &OpenOptions { read: true, write: false, append: false, truncate: false, create: false, create_new: false })?;
    let writer = File::open(to, &OpenOptions { read: false, write: true, append: false, truncate: true, create: true, create_new: false })?;
    let mut buf = vec![0u8; 8192];
    let mut total = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 { break; }
        writer.write(&buf[..n])?;
        total += n as u64;
    }
    Ok(total)
}

pub fn set_times(_p: &Path, _times: FileTimes) -> io::Result<()> {
    Ok(())
}

pub fn set_times_nofollow(_p: &Path, _times: FileTimes) -> io::Result<()> {
    Ok(())
}
