//! Type-safe kernel message queue API for inter-process communication.
//!
//! Messages carry a `msg_type` tag and an optional typed payload. The payload
//! is heap-allocated by the sender and freed by the receiver — safe because
//! all ToyOS processes share the same address space.

/// A message passed between processes via the kernel message queue.
///
/// Payload fields are private. Use [`Message::new`] to create a message with
/// a typed payload, and [`Message::take_payload`] to extract it on the
/// receiving side. Std validates that the payload size matches.
#[repr(C)]
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub struct Message {
    sender: u32,
    msg_type: u32,
    data: u64,
    len: u64,
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl Message {
    /// Create a message with a typed payload. The payload is heap-allocated
    /// and will be freed when the receiver calls [`take_payload`].
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn new<T>(msg_type: u32, payload: T) -> Self {
        let len = core::mem::size_of::<T>();
        let data = Box::into_raw(Box::new(payload)) as u64;
        Self { sender: 0, msg_type, data, len: len as u64 }
    }

    /// Create a message with no payload.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn signal(msg_type: u32) -> Self {
        Self { sender: 0, msg_type, data: 0, len: 0 }
    }

    /// PID of the sender (set by the kernel).
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn sender(&self) -> u32 {
        self.sender
    }

    /// Application-defined message type tag.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn msg_type(&self) -> u32 {
        self.msg_type
    }

    /// Extract the typed payload, consuming the message.
    /// Panics if `size_of::<T>()` doesn't match the payload size.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn take_payload<T>(self) -> T {
        let expected = core::mem::size_of::<T>() as u64;
        assert_eq!(
            self.len, expected,
            "message payload size mismatch: expected {expected}, got {}",
            self.len,
        );
        let ptr: *mut T = core::ptr::with_exposed_provenance_mut(self.data as usize);
        let value = *unsafe { Box::from_raw(ptr) };
        core::mem::forget(self);
        value
    }
}

/// Send a message to another process. Panics on failure.
/// Consumes the message — payload ownership transfers to the receiver.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn send(target_pid: u32, msg: Message) {
    let result = crate::sys::send_msg(target_pid as u64, &msg as *const Message as u64);
    core::mem::forget(msg);
    assert_eq!(result, 0, "failed to send message to pid {target_pid}");
}

/// Receive the next message from this process's queue. Blocks if empty.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn recv() -> Message {
    let mut msg = Message { sender: 0, msg_type: 0, data: 0, len: 0 };
    crate::sys::recv_msg(&mut msg as *mut Message as u64);
    msg
}
