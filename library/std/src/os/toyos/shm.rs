//! Shared memory with RAII.

use toyos_abi::Pid;
use toyos_abi::syscall;

/// A shared memory region with automatic cleanup.
///
/// When dropped, the region is unmapped and released.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub struct SharedMemory {
    token: u32,
    ptr: *mut u8,
    size: usize,
}

// SharedMemory contains a raw pointer but is safe to send between threads —
// the kernel manages the underlying mapping per-process.
#[stable(feature = "toyos_ext", since = "1.0.0")]
unsafe impl Send for SharedMemory {}
#[stable(feature = "toyos_ext", since = "1.0.0")]
unsafe impl Sync for SharedMemory {}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl SharedMemory {
    /// Allocate a new shared memory region and map it into this process.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn allocate(size: usize) -> Self {
        let token = syscall::alloc_shared(size);
        // SAFETY: token is valid, just allocated above.
        let ptr = unsafe { syscall::map_shared(token) };
        assert!(!ptr.is_null(), "map_shared failed");
        Self { token, ptr, size }
    }

    /// Map an existing shared memory region by token.
    ///
    /// The caller must know the region size (typically received via IPC
    /// alongside the token).
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn map(token: u32, size: usize) -> Self {
        // SAFETY: token must be valid and granted to this process.
        let ptr = unsafe { syscall::map_shared(token) };
        assert!(!ptr.is_null(), "map_shared failed");
        Self { token, ptr, size }
    }

    /// The opaque token identifying this shared memory region.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn token(&self) -> u32 {
        self.token
    }

    /// Grant another process permission to map this region.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn grant(&self, pid: u32) {
        syscall::grant_shared(self.token, Pid(pid));
    }

    /// Raw pointer to the mapped memory.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn as_ptr(&self) -> *mut u8 {
        self.ptr
    }

    /// Size of the region in bytes.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn len(&self) -> usize {
        self.size
    }

    /// View the region as a byte slice.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: ptr is valid and mapped for `size` bytes.
        unsafe { core::slice::from_raw_parts(self.ptr, self.size) }
    }

    /// View the region as a mutable byte slice.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: ptr is valid, mapped, and we have exclusive access.
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.size) }
    }
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl Drop for SharedMemory {
    fn drop(&mut self) {
        syscall::release_shared(self.token);
    }
}
