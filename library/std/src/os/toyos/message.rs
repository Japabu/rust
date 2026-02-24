//! Kernel message queue API for inter-process communication.

/// A message passed between processes via the kernel message queue.
#[repr(C)]
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub struct Message {
    /// PID of the sender (set by kernel on receive).
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub sender: u32,
    /// Application-defined message type.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub msg_type: u32,
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub a: u64,
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub b: u64,
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub c: u64,
}

/// Send a message to another process. Returns true on success.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn send(target_pid: u32, msg: &Message) -> bool {
    crate::sys::send_msg(target_pid as u64, msg as *const Message as u64) == 0
}

/// Receive the next message from this process's queue. Blocks if empty.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn recv() -> Message {
    let mut msg = Message { sender: 0, msg_type: 0, a: 0, b: 0, c: 0 };
    crate::sys::recv_msg(&mut msg as *mut Message as u64);
    msg
}
