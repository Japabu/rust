pub mod futex;
pub mod os;

#[expect(dead_code)]
#[path = "../unsupported/common.rs"]
mod unsupported_common;

pub use unsupported_common::{cleanup, init};

use core::sync::atomic::{AtomicUsize, Ordering};

// argc/argv stored by _start for std::env::args()
pub(crate) static ARGC: AtomicUsize = AtomicUsize::new(0);
pub(crate) static ARGV: AtomicUsize = AtomicUsize::new(0); // *const *const u8 as usize

#[unsafe(no_mangle)]
#[unsafe(naked)]
unsafe extern "C" fn _start() -> ! {
    // Stack layout at entry (set up by kernel):
    //   [RSP]   = argc
    //   [RSP+8] = argv[0], argv[1], ..., NULL
    core::arch::naked_asm!(
        "mov rdi, [rsp]",
        "lea rsi, [rsp + 8]",
        "call {start_rust}",
        "ud2",
        start_rust = sym start_rust,
    );
}

extern "C" fn start_rust(argc: usize, argv: *const *const u8) -> ! {
    unsafe extern "C" {
        fn main(argc: i32, argv: *const *const u8) -> i32;
    }
    ARGC.store(argc, Ordering::Relaxed);
    ARGV.store(argv as usize, Ordering::Relaxed);

    // Initialize environment variables and seed defaults
    crate::sys::env::init();
    unsafe {
        crate::sys::env::setenv("HOME".as_ref(), "/home/root".as_ref()).ok();
        crate::sys::env::setenv("XDG_CONFIG_HOME".as_ref(), "/home/root/.config".as_ref()).ok();
    }

    let code = unsafe { main(argc as i32, argv) };
    toyos_abi::syscall::exit(code)
}

pub fn abort_internal() -> ! {
    toyos_abi::syscall::exit(128 + 6) // SIGABRT-like — kill entire process
}

// C allocator shims — many crates (zlib-rs, etc.) call malloc/free/calloc
// via extern "C". Route through the Rust global allocator (arena+slab)
// to avoid per-allocation syscalls.
mod c_allocator {
    use crate::alloc::{GlobalAlloc, Layout, System};

    const HEADER: usize = 16; // stores the allocation size for free/realloc
    const ALIGN: usize = 16;

    #[unsafe(no_mangle)]
    unsafe extern "C" fn malloc(size: usize) -> *mut u8 {
        if size == 0 {
            return core::ptr::null_mut();
        }
        let total = HEADER + size;
        let layout = unsafe { Layout::from_size_align_unchecked(total, ALIGN) };
        let ptr = unsafe { System.alloc(layout) };
        if ptr.is_null() {
            return ptr;
        }
        unsafe { (ptr as *mut usize).write(total) };
        unsafe { ptr.add(HEADER) }
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn calloc(count: usize, size: usize) -> *mut u8 {
        let total = count.saturating_mul(size);
        let ptr = malloc(total);
        if !ptr.is_null() && total > 0 {
            unsafe { core::ptr::write_bytes(ptr, 0, total) };
        }
        ptr
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn free(ptr: *mut u8) {
        if ptr.is_null() {
            return;
        }
        let base = unsafe { ptr.sub(HEADER) };
        let total = unsafe { (base as *const usize).read() };
        let layout = unsafe { Layout::from_size_align_unchecked(total, ALIGN) };
        unsafe { System.dealloc(base, layout) };
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn realloc(ptr: *mut u8, new_size: usize) -> *mut u8 {
        if ptr.is_null() {
            return malloc(new_size);
        }
        if new_size == 0 {
            free(ptr);
            return core::ptr::null_mut();
        }
        let base = unsafe { ptr.sub(HEADER) };
        let old_total = unsafe { (base as *const usize).read() };
        let new_total = HEADER + new_size;
        let old_layout = unsafe { Layout::from_size_align_unchecked(old_total, ALIGN) };
        let new_base = unsafe { System.realloc(base, old_layout, new_total) };
        if new_base.is_null() {
            return new_base;
        }
        unsafe { (new_base as *mut usize).write(new_total) };
        unsafe { new_base.add(HEADER) }
    }
}
