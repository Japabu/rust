use crate::sys::AsInner;

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

/// Get the screen size as (rows, columns).
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn screen_size() -> (usize, usize) {
    crate::sys::stdio::screen_size()
}

/// Set the screen size from pixel dimensions (width, height).
/// The kernel computes rows/columns assuming an 8x16 font.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn set_screen_size(width: u32, height: u32) {
    crate::sys::stdio::set_screen_size(width, height);
}

/// Transfer a region of the framebuffer to the GPU and flush it.
/// Pass (0, 0, 0, 0) to flush the full screen.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn gpu_present(x: u32, y: u32, w: u32, h: u32) {
    crate::sys::stdio::gpu_present(x, y, w, h);
}

/// Upload the cursor image from backing and enable hardware cursor.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn gpu_set_cursor(hot_x: u32, hot_y: u32) {
    crate::sys::stdio::gpu_set_cursor(hot_x, hot_y);
}

/// Move the hardware cursor to screen position (x, y).
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn gpu_move_cursor(x: u32, y: u32) {
    crate::sys::stdio::gpu_move_cursor(x, y);
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

/// Result of a [`poll`] call.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub struct PollResult {
    mask: u64,
    fd_count: usize,
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl PollResult {
    /// Whether the file descriptor at `index` is ready.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn fd(&self, index: usize) -> bool {
        self.mask & (1 << index) != 0
    }

    /// Whether the process message queue has messages.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn messages(&self) -> bool {
        self.mask & (1 << self.fd_count) != 0
    }
}

/// Poll file descriptors and the message queue for readiness.
/// Blocks until at least one source has data.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn poll(fds: &[u64]) -> PollResult {
    poll_timeout(fds, 0)
}

/// Poll file descriptors and the message queue for readiness.
/// Returns when at least one source has data, or after `timeout_nanos`
/// nanoseconds (whichever comes first). Pass 0 to block indefinitely.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn poll_timeout(fds: &[u64], timeout_nanos: u64) -> PollResult {
    let mask = crate::sys::stdio::poll(fds.as_ptr() as u64, fds.len() as u64, timeout_nanos);
    PollResult { mask, fd_count: fds.len() }
}

/// Read from a file descriptor. Returns bytes read.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn read_fd(fd: u64, buf: &mut [u8]) -> usize {
    crate::sys::stdio::read_fd(fd, buf.as_mut_ptr(), buf.len()) as usize
}

/// Get the raw FD number from a std type.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub trait AsRawFd {
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    fn as_raw_fd(&self) -> u64;
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl AsRawFd for crate::process::ChildStdin {
    fn as_raw_fd(&self) -> u64 {
        self.as_inner().raw_fd()
    }
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl AsRawFd for crate::process::ChildStdout {
    fn as_raw_fd(&self) -> u64 {
        self.as_inner().raw_fd()
    }
}

// --- Device access ---

/// Device types for [`open_device`].
#[stable(feature = "toyos_ext", since = "1.0.0")]
#[repr(u64)]
#[derive(Debug, Clone, Copy)]
pub enum DeviceType {
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    Keyboard = 0,
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    Mouse = 1,
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    Framebuffer = 2,
}

/// Claim exclusive access to a device. Returns the FD number on success.
/// Fails if the device is already claimed by another process.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn open_device(device: DeviceType) -> Option<u64> {
    let fd = crate::sys::open_device(device as u64);
    if fd == u64::MAX { None } else { Some(fd) }
}

// --- Name registry ---

/// Register the current process under the given name.
/// Fails if the name is already taken by another process.
/// Other processes can discover this process via [`find_pid`].
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn register_name(name: &str) -> Result<(), NameTaken> {
    let result = crate::sys::register_name(name.as_ptr(), name.len());
    if result == 0 { Ok(()) } else { Err(NameTaken) }
}

/// Error returned when [`register_name`] fails because the name is already registered.
#[stable(feature = "toyos_ext", since = "1.0.0")]
#[derive(Debug)]
pub struct NameTaken;

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl core::fmt::Display for NameTaken {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("name already registered")
    }
}

/// Find the PID of a process registered under the given name.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn find_pid(name: &str) -> Option<u32> {
    let pid = crate::sys::find_pid(name.as_ptr(), name.len());
    if pid == u64::MAX { None } else { Some(pid as u32) }
}

// --- Shared memory ---

/// Allocate a 2MB-aligned shared memory region. Returns an opaque token.
/// The region is mapped into the caller's address space automatically.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn alloc_shared(size: usize) -> u32 {
    let token = crate::sys::alloc_shared(size as u64);
    assert!(token != u64::MAX, "alloc_shared failed");
    token as u32
}

/// Grant another process permission to map a shared memory region.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn grant_shared(token: u32, target_pid: u32) {
    let result = crate::sys::grant_shared(token as u64, target_pid as u64);
    assert_eq!(result, 0, "grant_shared failed");
}

/// Map a shared memory region into this process's address space.
/// Returns a pointer to the mapped memory.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn map_shared(token: u32) -> *mut u8 {
    let addr = crate::sys::map_shared(token as u64);
    assert!(addr != u64::MAX, "map_shared failed");
    core::ptr::with_exposed_provenance_mut(addr as usize)
}

/// Free a shared memory region owned by the caller.
/// Unmaps from all processes that mapped it and deallocates the backing memory.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn free_shared(token: u32) {
    let result = crate::sys::free_shared(token as u64);
    assert_eq!(result, 0, "free_shared failed");
}

/// Query system information (memory, CPUs, processes).
/// Fills `buf` with a header followed by per-process entries.
/// Returns the number of bytes written.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn sysinfo(buf: &mut [u8]) -> usize {
    let n = crate::sys::stdio::sysinfo(buf.as_mut_ptr(), buf.len());
    if n == u64::MAX { 0 } else { n as usize }
}

/// Nanoseconds since boot (monotonic clock).
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn clock_nanos() -> u64 {
    crate::sys::clock()
}

/// Read wall-clock time from RTC.
/// Returns packed: (hours << 16) | (minutes << 8) | seconds.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn clock_realtime() -> u64 {
    crate::sys::clock_realtime()
}
