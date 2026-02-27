use crate::ffi::OsString;
use crate::fmt;
use crate::fs::TryLockError;
use crate::hash::Hash;
use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut, SeekFrom};
use crate::path::{Path, PathBuf};
pub use crate::sys::fs::common::Dir;
use crate::sys::time::SystemTime;

// Open flags (must match kernel)
const O_READ: u64 = 1;
const O_WRITE: u64 = 2;
const O_CREATE: u64 = 4;
const O_TRUNCATE: u64 = 8;

pub struct File(u64); // file descriptor

#[derive(Clone)]
pub struct FileAttr {
    size: u64,
    file_type: u64, // 1 = regular file, 2 = directory
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
            is_file: self.file_type == 1,
            is_dir: self.file_type == 2,
        }
    }

    pub fn modified(&self) -> io::Result<SystemTime> {
        Ok(SystemTime::now())
    }

    pub fn accessed(&self) -> io::Result<SystemTime> {
        Ok(SystemTime::now())
    }

    pub fn created(&self) -> io::Result<SystemTime> {
        Ok(SystemTime::now())
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
            // Reconstruct entry (we can't move out of Vec while iterating)
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
            file_type: if self.is_dir { 2 } else { 1 },
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

    fn to_flags(&self) -> u64 {
        let mut flags = 0u64;
        if self.read { flags |= O_READ; }
        if self.write || self.append { flags |= O_WRITE; }
        if self.create || self.create_new { flags |= O_CREATE; }
        if self.truncate { flags |= O_TRUNCATE; }
        flags
    }
}

impl File {
    pub fn raw_fd(&self) -> u64 {
        self.0
    }

    pub fn open(path: &Path, opts: &OpenOptions) -> io::Result<File> {
        let flags = opts.to_flags();
        let path_bytes = path.as_os_str().as_encoded_bytes();
        let fd = toyos_abi::syscall::open(path_bytes.as_ptr(), path_bytes.len(), flags);
        if fd == u64::MAX {
            Err(io::Error::new(io::ErrorKind::NotFound, "file not found"))
        } else {
            Ok(File(fd))
        }
    }

    pub fn file_attr(&self) -> io::Result<FileAttr> {
        let result = toyos_abi::syscall::fstat(self.0);
        if result == u64::MAX {
            return Err(io::Error::new(io::ErrorKind::Other, "fstat failed"));
        }
        let file_type = result >> 32;
        let size = result & 0xFFFF_FFFF;
        Ok(FileAttr { size, file_type })
    }

    pub fn fsync(&self) -> io::Result<()> {
        let r = toyos_abi::syscall::fsync(self.0);
        if r == u64::MAX {
            Err(io::Error::new(io::ErrorKind::Other, "fsync failed"))
        } else {
            Ok(())
        }
    }

    pub fn datasync(&self) -> io::Result<()> {
        self.fsync()
    }

    pub fn lock(&self) -> io::Result<()> { Ok(()) }
    pub fn lock_shared(&self) -> io::Result<()> { Ok(()) }
    pub fn try_lock(&self) -> Result<(), TryLockError> { Ok(()) }
    pub fn try_lock_shared(&self) -> Result<(), TryLockError> { Ok(()) }
    pub fn unlock(&self) -> io::Result<()> { Ok(()) }

    pub fn truncate(&self, _size: u64) -> io::Result<()> {
        panic!("File::truncate not implemented")
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let n = toyos_abi::syscall::read(self.0, buf.as_mut_ptr(), buf.len());
        if n == u64::MAX {
            Err(io::Error::new(io::ErrorKind::Other, "read failed"))
        } else {
            Ok(n as usize)
        }
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
        let n = toyos_abi::syscall::write(self.0, buf.as_ptr(), buf.len());
        if n == u64::MAX {
            Err(io::Error::new(io::ErrorKind::Other, "write failed"))
        } else {
            Ok(n as usize)
        }
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
        let (offset, whence) = match pos {
            SeekFrom::Start(n) => (n as i64, 0u64),
            SeekFrom::Current(n) => (n, 1u64),
            SeekFrom::End(n) => (n, 2u64),
        };
        let result = toyos_abi::syscall::seek(self.0, offset, whence);
        if result == u64::MAX {
            Err(io::Error::new(io::ErrorKind::Other, "seek failed"))
        } else {
            Ok(result)
        }
    }

    pub fn size(&self) -> Option<io::Result<u64>> {
        Some(self.file_attr().map(|a| a.size))
    }

    pub fn tell(&self) -> io::Result<u64> {
        self.seek(SeekFrom::Current(0))
    }

    pub fn duplicate(&self) -> io::Result<File> {
        panic!("File::duplicate not implemented")
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
        toyos_abi::syscall::close(self.0);
    }
}

impl DirBuilder {
    pub fn new() -> DirBuilder {
        DirBuilder {}
    }

    pub fn mkdir(&self, _p: &Path) -> io::Result<()> {
        Ok(()) // Directories are virtual (derived from file path prefixes)
    }
}

impl fmt::Debug for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "File({})", self.0)
    }
}

pub fn readdir(p: &Path) -> io::Result<ReadDir> {
    let path_bytes = p.as_os_str().as_encoded_bytes();
    let mut buf = vec![0u8; 65536];
    let n = toyos_abi::syscall::readdir(
        path_bytes.as_ptr(),
        path_bytes.len(),
        buf.as_mut_ptr(),
        buf.len(),
    );
    if n == u64::MAX {
        return Err(io::Error::new(io::ErrorKind::NotFound, "directory not found"));
    }

    // Parse entries: type_u8 name_bytes \0 size_u64_le
    let data = &buf[..n as usize];
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
    let result = toyos_abi::syscall::delete(path_bytes.as_ptr(), path_bytes.len());
    if result == u64::MAX {
        Err(io::Error::new(io::ErrorKind::NotFound, "file not found"))
    } else {
        Ok(())
    }
}

pub fn rename(_old: &Path, _new: &Path) -> io::Result<()> {
    panic!("rename not implemented")
}

pub fn set_perm(_p: &Path, _perm: FilePermissions) -> io::Result<()> {
    Ok(())
}

pub fn rmdir(_p: &Path) -> io::Result<()> {
    panic!("rmdir not implemented")
}

pub fn remove_dir_all(_path: &Path) -> io::Result<()> {
    panic!("remove_dir_all not implemented")
}

pub fn exists(path: &Path) -> io::Result<bool> {
    let path_bytes = path.as_os_str().as_encoded_bytes();
    let fd = toyos_abi::syscall::open(path_bytes.as_ptr(), path_bytes.len(), O_READ);
    if fd == u64::MAX {
        Ok(false)
    } else {
        toyos_abi::syscall::close(fd);
        Ok(true)
    }
}

pub fn readlink(_p: &Path) -> io::Result<PathBuf> {
    panic!("readlink not implemented")
}

pub fn symlink(_original: &Path, _link: &Path) -> io::Result<()> {
    panic!("symlink not implemented")
}

pub fn link(_src: &Path, _dst: &Path) -> io::Result<()> {
    panic!("link not implemented")
}

pub fn stat(path: &Path) -> io::Result<FileAttr> {
    let path_bytes = path.as_os_str().as_encoded_bytes();
    let fd = toyos_abi::syscall::open(path_bytes.as_ptr(), path_bytes.len(), O_READ);
    if fd == u64::MAX {
        return Err(io::Error::new(io::ErrorKind::NotFound, "file not found"));
    }
    let result = toyos_abi::syscall::fstat(fd);
    toyos_abi::syscall::close(fd);
    if result == u64::MAX {
        return Err(io::Error::new(io::ErrorKind::Other, "fstat failed"));
    }
    let file_type = result >> 32;
    let size = result & 0xFFFF_FFFF;
    Ok(FileAttr { size, file_type })
}

pub fn lstat(p: &Path) -> io::Result<FileAttr> {
    stat(p) // no symlinks on toyos
}

pub fn canonicalize(_p: &Path) -> io::Result<PathBuf> {
    panic!("canonicalize not implemented")
}

pub fn copy(_from: &Path, _to: &Path) -> io::Result<u64> {
    panic!("copy not implemented")
}

pub fn set_times(_p: &Path, _times: FileTimes) -> io::Result<()> {
    panic!("set_times not implemented")
}

pub fn set_times_nofollow(_p: &Path, _times: FileTimes) -> io::Result<()> {
    panic!("set_times_nofollow not implemented")
}
