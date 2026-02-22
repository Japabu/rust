/// Switch stdin between canonical (line-buffered with echo) and raw mode.
///
/// In canonical mode (the default), `Stdin::read()` echoes characters, handles
/// backspace, and buffers a complete line before returning.
///
/// In raw mode, `Stdin::read()` passes bytes through directly from the kernel.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn set_stdin_raw(raw: bool) {
    crate::sys::stdio::set_stdin_raw(raw);
}

/// Read from stdin directly, bypassing `BufReader` buffering.
///
/// Use this for raw byte-at-a-time input (e.g. in a readline loop with
/// `set_stdin_raw(true)`).
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn read_stdin_raw(buf: &mut [u8]) -> crate::io::Result<usize> {
    crate::sys::stdio::read_stdin_raw(buf)
}

/// Get the screen size as (rows, columns).
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn screen_size() -> (usize, usize) {
    crate::sys::stdio::screen_size()
}

/// Set the active keyboard layout by name. Returns `true` on success.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn set_keyboard_layout(name: &str) -> bool {
    crate::sys::stdio::set_keyboard_layout(name)
}

/// Shut down the machine. Does not return.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn shutdown() -> ! {
    crate::sys::stdio::shutdown()
}
