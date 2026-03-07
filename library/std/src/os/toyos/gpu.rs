//! GPU and screen operations.

use toyos_abi::syscall;

/// Get the screen size as (width, height) in pixels.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn screen_size() -> (usize, usize) {
    syscall::screen_size()
}

/// Set the screen resolution.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn set_screen_size(width: u32, height: u32) {
    syscall::set_screen_size(width, height);
}

/// Flush a screen region to the display. Pass `(0, 0, 0, 0)` for full screen.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn present(x: u32, y: u32, w: u32, h: u32) {
    syscall::gpu_present(x, y, w, h);
}

/// Upload the cursor image from the cursor backing buffer and enable hardware cursor.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn set_cursor(hot_x: u32, hot_y: u32) {
    syscall::gpu_set_cursor(hot_x, hot_y);
}

/// Move the hardware cursor to screen position (x, y).
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn move_cursor(x: u32, y: u32) {
    syscall::gpu_move_cursor(x, y);
}
