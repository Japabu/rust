/// DTV-based TLS access for shared libraries (x86-64 GD/LD model).
///
/// Called by shared library code when accessing `#[thread_local]` variables.
/// The linker preserves `call __tls_get_addr` in .so files and emits
/// R_X86_64_DTPMOD64/DTPOFF64 GOT slot pairs. At load time, the kernel fills:
///   GOT[0] = module_id (DTV index, 1-based)
///   GOT[1] = offset within module's TLS segment
///
/// TCB layout (at fs_base / TP):
///   fs:[0x00] = self_ptr
///   fs:[0x08] = dtv_ptr (physical address of DTV)
///
/// DTV layout:
///   [0x00] generation: u64
///   [0x08] len: u64
///   [0x10] entries[0]: u64 (module_id=1)
///   [0x18] entries[1]: u64 (module_id=2)
///   ...
///
/// Entry value is the base address of that module's TLS block,
/// or DTV_UNALLOCATED (!0) if not yet allocated.
///
/// __tls_get_addr receives a pointer to TlsIndex {module_id, offset} in %rdi,
/// returns the address of the TLS variable in %rax.

/// Slow path for __tls_get_addr: the DTV entry is unallocated or out of range.
/// Calls SYS_TLS_ALLOC_BLOCK to allocate the TLS block on demand.
#[inline(never)]
unsafe extern "C" fn __tls_get_addr_slow(module_id: u64, offset: u64) -> *mut u8 {
    // `__tls_get_addr`'s ABI is an address and there is nobody to return an
    // error to: a refusal added to `offset` is a pointer near the top of the
    // address space that the caller would then dereference.
    match toyos_abi::syscall::tls_alloc_block(module_id) {
        Ok(block) => core::ptr::without_provenance_mut((block + offset) as usize),
        Err(_) => crate::rtabort!("no TLS block for a dlopen'd module"),
    }
}

/// Fast path: naked asm reads DTV directly from fs:[8], checks bounds and allocation,
/// falls through to slow path only when needed.
#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn __tls_get_addr(ti: *const [u64; 2]) -> *mut u8 {
    core::arch::naked_asm!(
        // ti is in %rdi: [module_id, offset]
        "mov rsi, [rdi + 8]",   // rsi = offset
        "mov rdi, [rdi]",       // rdi = module_id

        // module_id == 0 guard (shouldn't happen, but be safe)
        "test rdi, rdi",
        "jz 2f",

        // Load DTV pointer from TCB: fs:[8]
        "mov rax, fs:[8]",

        // Bounds check: module_id <= dtv[1] (len)
        "cmp rdi, [rax + 8]",
        "ja 2f",

        // Load DTV entry: dtv[2 + (module_id - 1)]
        "lea rcx, [rdi - 1]",
        "mov rax, [rax + rcx * 8 + 16]",

        // Check for DTV_UNALLOCATED (!0)
        "cmp rax, -1",
        "je 2f",

        // Fast path: return entry + offset
        "add rax, rsi",
        "ret",

        // Slow path
        "2:",
        "jmp {slow}",
        slow = sym __tls_get_addr_slow,
    );
}
