pub mod futex;
pub mod os;
pub mod tls;

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

/// .init_array constructor: registers the EH frame finder for DWARF unwinding.
/// For executables, this runs before `_start`. For cdylib .so files loaded via
/// dlopen, the kernel returns the .init_array to userspace which calls it.
/// This ensures panic unwinding works from code inside shared libraries.
extern "C" fn init_eh_frame() {
    eh_frame::init();
}

#[used]
#[unsafe(link_section = ".init_array")]
static INIT_EH_FRAME: extern "C" fn() = init_eh_frame;

extern "C" fn start_rust(argc: usize, argv: *const *const u8) -> ! {
    unsafe extern "C" {
        fn main(argc: i32, argv: *const *const u8) -> i32;
    }
    ARGC.store(argc, Ordering::Relaxed);
    ARGV.store(argv.expose_provenance(), Ordering::Relaxed);

    // Register EH frame finder (also in .init_array for cdylib, but exes don't run .init_array)
    eh_frame::init();

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

/// DWARF EH frame finder for the `unwinding` crate.
/// Locates `.eh_frame_hdr` for a given PC via `SYS_QUERY_MODULES`.
mod eh_frame {
    use crate::sync::Mutex;
    use toyos_abi::syscall::ModuleInfo;

    struct Module {
        base: usize,
        end: usize,
        eh_frame_hdr: usize,
        eh_frame_hdr_size: usize,
    }

    static CACHE: Mutex<Vec<Module>> = Mutex::new(Vec::new());

    fn load_modules() -> Vec<Module> {
        let mut buf = vec![0u8; 4096];
        loop {
            match toyos_abi::syscall::query_modules(&mut buf) {
                Ok(count) => {
                    let info_size = core::mem::size_of::<ModuleInfo>();
                    let mut modules = Vec::with_capacity(count);
                    for i in 0..count {
                        let off = i * info_size;
                        if off + info_size > buf.len() { break; }
                        let info = unsafe { &*(buf.as_ptr().add(off) as *const ModuleInfo) };
                        modules.push(Module {
                            base: info.base as usize,
                            end: info.text_end as usize,
                            eh_frame_hdr: info.eh_frame_hdr as usize,
                            eh_frame_hdr_size: info.eh_frame_hdr_size as usize,
                        });
                    }
                    return modules;
                }
                Err(_) => {
                    buf.resize(buf.len() * 2, 0);
                    if buf.len() > 1024 * 1024 { return Vec::new(); }
                }
            }
        }
    }

    struct ToyOsEhFrameFinder;
    static FINDER: ToyOsEhFrameFinder = ToyOsEhFrameFinder;

    unsafe impl unwind::EhFrameFinder for ToyOsEhFrameFinder {
        fn find(&self, pc: usize) -> Option<unwind::FrameInfo> {
            // Fast path: check cached modules
            {
                let cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(m) = cache.iter().find(|m| pc >= m.base && pc < m.end) {
                    if m.eh_frame_hdr != 0 {
                        return Some(unwind::FrameInfo {
                            text_base: Some(m.base),
                            kind: unwind::FrameInfoKind::EhFrameHdr(m.eh_frame_hdr),
                        });
                    }
                    return None;
                }
            }

            // Cache miss — reload module list (handles dlopen)
            let modules = load_modules();
            let result = modules.iter().find(|m| pc >= m.base && pc < m.end).and_then(|m| {
                if m.eh_frame_hdr != 0 {
                    Some(unwind::FrameInfo {
                        text_base: Some(m.base),
                        kind: unwind::FrameInfoKind::EhFrameHdr(m.eh_frame_hdr),
                    })
                } else {
                    None
                }
            });
            *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = modules;
            result
        }
    }

    pub(super) fn init() {
        let modules = load_modules();
        *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = modules;
        unwind::set_custom_eh_frame_finder(&FINDER).ok();
    }
}
