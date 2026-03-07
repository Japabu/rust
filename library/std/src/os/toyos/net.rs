//! Raw Ethernet frame access (for the network daemon).

use crate::time::Duration;
use toyos_abi::syscall;

/// Get the MAC address of the network interface.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn mac_address() -> Option<[u8; 6]> {
    syscall::net_mac()
}

/// Send a raw Ethernet frame.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn send_frame(frame: &[u8]) {
    syscall::net_send(frame);
}

/// Receive a raw Ethernet frame. Blocks until a frame arrives.
/// Returns the number of bytes written to `buf`.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn recv_frame(buf: &mut [u8]) -> usize {
    syscall::net_recv(buf)
}

/// Receive a raw Ethernet frame with a timeout.
/// Returns the number of bytes written, or 0 on timeout.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn recv_frame_timeout(buf: &mut [u8], timeout: Option<Duration>) -> usize {
    let nanos = timeout.map(|d| d.as_nanos() as u64);
    syscall::net_recv_timeout(buf, nanos)
}

/// Poll for a received frame in the DMA buffer (zero-copy path).
/// Returns `Some((buf_index, frame_len))` or `None`.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn nic_rx_poll() -> Option<(usize, usize)> {
    let v = syscall::nic_rx_poll();
    if v == 0 { None } else { Some(((v >> 16) as usize, (v & 0xFFFF) as usize)) }
}

/// Refill an RX DMA buffer after consuming the frame.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn nic_rx_done(buf_index: usize) {
    syscall::nic_rx_done(buf_index as u64);
}

/// Submit the TX DMA buffer to hardware. `total_len` includes the net header.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn nic_tx(total_len: usize) {
    syscall::nic_tx(total_len as u64);
}
