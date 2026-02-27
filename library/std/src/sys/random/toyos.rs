pub fn fill_bytes(buf: &mut [u8]) {
    toyos_abi::syscall::random(buf)
}
