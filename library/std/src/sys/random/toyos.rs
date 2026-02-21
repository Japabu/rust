pub fn fill_bytes(buf: &mut [u8]) {
    crate::sys::pal::random(buf.as_mut_ptr(), buf.len())
}
