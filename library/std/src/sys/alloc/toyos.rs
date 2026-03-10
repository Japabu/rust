use crate::alloc::{GlobalAlloc, Layout, System};
use toyos_abi::syscall::{self, MmapFlags, MmapProt};

use core::ptr;
use core::sync::atomic::{AtomicU64, Ordering};

/// Chunk size for the arena allocator. Larger = fewer mmap syscalls.
/// The kernel rounds mmap sizes up to 2MB pages, so this is the effective
/// granularity. Using 64MB reduces syscall overhead for allocation-heavy
/// programs like the Rust compiler.
const ARENA_SIZE: usize = 64 * 1024 * 1024;

/// Allocations larger than this go directly through mmap.
const LARGE_THRESHOLD: usize = 32 * 1024 * 1024;

/// Single-threaded bump arena for small allocations.
/// Using atomics for interior mutability (GlobalAlloc requires &self).
struct Arena {
    /// Current bump pointer (0 = no arena allocated yet).
    cursor: AtomicU64,
    /// End of current arena chunk.
    end: AtomicU64,
}

impl Arena {
    const fn new() -> Self {
        Self {
            cursor: AtomicU64::new(0),
            end: AtomicU64::new(0),
        }
    }
}

static ARENA: Arena = Arena::new();

/// Bump-allocate from the arena. Returns null if mmap fails.
fn arena_alloc(size: usize, align: usize) -> *mut u8 {
    loop {
        let cursor = ARENA.cursor.load(Ordering::Acquire);
        let end = ARENA.end.load(Ordering::Acquire);

        if cursor == 0 || end == 0 {
            let arena = unsafe { syscall::mmap(
                ptr::null_mut(), ARENA_SIZE,
                MmapProt::READ | MmapProt::WRITE, MmapFlags::ANONYMOUS,
            ) };
            if arena.is_null() { return ptr::null_mut(); }
            let start = arena as u64;
            ARENA.cursor.store(start, Ordering::Release);
            ARENA.end.store(start + ARENA_SIZE as u64, Ordering::Release);
            continue;
        }

        let aligned = (cursor as usize + align - 1) & !(align - 1);
        let new_cursor = aligned + size;

        if new_cursor as u64 > end {
            let arena = unsafe { syscall::mmap(
                ptr::null_mut(), ARENA_SIZE,
                MmapProt::READ | MmapProt::WRITE, MmapFlags::ANONYMOUS,
            ) };
            if arena.is_null() { return ptr::null_mut(); }
            let start = arena as u64;
            ARENA.cursor.store(start, Ordering::Release);
            ARENA.end.store(start + ARENA_SIZE as u64, Ordering::Release);
            continue;
        }

        match ARENA.cursor.compare_exchange_weak(
            cursor, new_cursor as u64,
            Ordering::AcqRel, Ordering::Relaxed,
        ) {
            Ok(_) => return ptr::with_exposed_provenance_mut(aligned),
            Err(_) => continue,
        }
    }
}

#[stable(feature = "alloc_system_type", since = "1.28.0")]
unsafe impl GlobalAlloc for System {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        let effective = size.max(align);

        // Large allocations: direct mmap
        if effective > LARGE_THRESHOLD {
            return unsafe { syscall::mmap(
                ptr::null_mut(), size,
                MmapProt::READ | MmapProt::WRITE, MmapFlags::ANONYMOUS,
            ) };
        }

        // Small allocations: bump allocate from arena (no syscall)
        arena_alloc(effective.max(8), align.max(8))
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() { return; }
        let size = layout.size();
        let align = layout.align();
        let effective = size.max(align);

        // Large allocations: munmap
        if effective > LARGE_THRESHOLD {
            unsafe { syscall::munmap(ptr, size); }
            return;
        }

        // Small allocations: leak (arena memory is never returned to OS)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let old_size = layout.size();
        let align = layout.align();
        let old_effective = old_size.max(align);
        let new_effective = new_size.max(align);

        // Shrinking is always a no-op
        if new_size <= old_size {
            return ptr;
        }

        // Alloc new, copy, free old
        let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, align) };
        let new_ptr = unsafe { self.alloc(new_layout) };
        if !new_ptr.is_null() {
            let copy_size = old_size.min(new_size);
            unsafe { ptr::copy_nonoverlapping(ptr, new_ptr, copy_size); }
            unsafe { self.dealloc(ptr, layout); }
        }
        new_ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // Both arena and mmap return zeroed memory
        unsafe { self.alloc(layout) }
    }
}
