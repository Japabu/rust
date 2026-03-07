use crate::alloc::{GlobalAlloc, Layout, System};
use toyos_abi::syscall;

#[stable(feature = "alloc_system_type", since = "1.28.0")]
unsafe impl GlobalAlloc for System {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: layout is valid per GlobalAlloc contract; syscall handles allocation.
        unsafe { syscall::alloc(layout.size(), layout.align()) }
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: ptr was allocated with matching layout per GlobalAlloc contract.
        unsafe { syscall::free(ptr, layout.size(), layout.align()) }
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: ptr was allocated with matching layout per GlobalAlloc contract.
        unsafe { syscall::realloc(ptr, layout.size(), layout.align(), new_size) }
    }
}
