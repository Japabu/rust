/// DTV-based TLS access for shared libraries (x86-64 GD model).
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

const DTV_UNALLOCATED: u64 = !0u64;

/// Slow path for __tls_get_addr: the DTV entry is unallocated.
/// Calls SYS_TLS_ALLOC_BLOCK to allocate the TLS block on demand and stores it in the DTV.
#[inline(never)]
unsafe extern "C" fn __tls_get_addr_slow(module_id: u64, offset: u64) -> *mut u8 {
    // Ask the kernel to allocate the TLS block and write it into our DTV.
    // Returns the physical address of the block (same address space as DTV entries).
    let block_phys = toyos_abi::syscall::tls_alloc_block(module_id);
    core::ptr::without_provenance_mut((block_phys + offset) as usize)
}

/// Fast path: naked asm that avoids function prologue overhead.
/// Receives TlsIndex* in %rdi, returns TLS variable address in %rax.
#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn __tls_get_addr() {
    // rdi = pointer to TlsIndex { module_id: u64, offset: u64 }
    core::arch::naked_asm!(
        // Load module_id and offset from TlsIndex
        "mov rsi, [rdi + 8]",    // rsi = offset
        "mov rdi, [rdi]",        // rdi = module_id

        // module_id must be >= 1 (1-based index)
        "test rdi, rdi",
        "jz 2f",

        // Load DTV pointer from TCB: fs:[8]
        "mov rax, fs:[8]",

        // Bounds check: module_id <= dtv.len (at dtv+8)
        "cmp rdi, [rax + 8]",
        "ja 2f",                 // module_id > len → slow path

        // Load entry: dtv + 16 + (module_id - 1) * 8
        "lea rcx, [rdi - 1]",
        "mov rax, [rax + 16 + rcx * 8]",

        // Check for DTV_UNALLOCATED (all-ones)
        "cmp rax, -1",
        "je 2f",                 // unallocated → slow path

        // Fast path: return entry + offset
        "add rax, rsi",
        "ret",

        // Slow path
        "2:",
        // rdi = module_id, rsi = offset (already in place)
        "call {slow}",
        "ret",
        slow = sym __tls_get_addr_slow,
    );
}
