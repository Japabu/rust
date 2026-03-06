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
        crate::sys::env::setenv("HOME".as_ref(), "/".as_ref()).ok();
        crate::sys::env::setenv("XDG_CONFIG_HOME".as_ref(), "/nvme/config".as_ref()).ok();
    }

    let code = unsafe { main(argc as i32, argv) };
    toyos_abi::syscall::exit_group(code)
}

pub fn abort_internal() -> ! {
    toyos_abi::syscall::exit_group(128 + 6) // SIGABRT-like — kill entire process
}
