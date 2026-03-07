//! System information and control.

use toyos_abi::syscall;

/// Wall-clock time from the hardware RTC.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub struct RealTime {
    /// Hours (0–23).
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub hours: u8,
    /// Minutes (0–59).
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub minutes: u8,
    /// Seconds (0–59).
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub seconds: u8,
}

/// Read the wall-clock time from the hardware RTC.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn clock_realtime() -> RealTime {
    let rt = syscall::clock_realtime();
    RealTime { hours: rt.hours, minutes: rt.minutes, seconds: rt.seconds }
}

/// Query system information (memory, CPU, processes) into `buf`.
/// Returns the number of bytes written.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn sysinfo(buf: &mut [u8]) -> usize {
    syscall::sysinfo(buf)
}

/// Return the number of available CPUs.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn cpu_count() -> u32 {
    syscall::cpu_count()
}

/// Shut down the machine. Does not return.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn shutdown() -> ! {
    syscall::shutdown()
}

/// Set the active keyboard layout by name. Returns `true` on success.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn set_keyboard_layout(name: &str) -> bool {
    syscall::set_keyboard_layout(name)
}
