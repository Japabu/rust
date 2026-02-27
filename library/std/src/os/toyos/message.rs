//! Type-safe kernel message queue API for inter-process communication.
//!
//! Messages carry a `msg_type` tag and an optional typed payload. The kernel
//! copies payload bytes between address spaces — sender and receiver don't
//! need to share memory.

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

    /// Create a message with a variable-length byte payload.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn from_bytes(msg_type: u32, bytes: &[u8]) -> Self {
        let boxed = bytes.to_vec().into_boxed_slice();
        let len = boxed.len();
        let data = Box::into_raw(boxed) as *mut u8 as u64;
        Self { sender: 0, msg_type, data, len: len as u64 }
    }

    /// Extract the payload as raw bytes, consuming the message.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn take_bytes(self) -> Vec<u8> {
        if self.data == 0 || self.len == 0 {
            core::mem::forget(self);
            return Vec::new();
        }
        let bytes = unsafe {
            core::slice::from_raw_parts(core::ptr::with_exposed_provenance(self.data as usize), self.len as usize)
        }.to_vec();
        toyos_abi::syscall::free(core::ptr::with_exposed_provenance_mut(self.data as usize), self.len as usize, 1);
        core::mem::forget(self);
        bytes
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
        let ptr: *const T = core::ptr::with_exposed_provenance(self.data as usize);
        let value = unsafe { core::ptr::read(ptr) };
        // Free the kernel-allocated buffer
        toyos_abi::syscall::free(core::ptr::with_exposed_provenance_mut(self.data as usize), self.len as usize, 1);
        core::mem::forget(self);
        value
    }
}

/// Send a message to another process. Panics on failure.
/// The kernel copies the payload bytes — the sender's allocation is freed after sending.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn send(target_pid: u32, msg: Message) {
    let result = toyos_abi::syscall::send_msg(target_pid as u64, &msg as *const Message as u64);
    // Free the sender's heap allocation — kernel has already copied the bytes
    if msg.data != 0 && msg.len != 0 {
        toyos_abi::syscall::free(core::ptr::with_exposed_provenance_mut(msg.data as usize), msg.len as usize, 1);
    }
    core::mem::forget(msg);
    assert_eq!(result, 0, "failed to send message to pid {target_pid}");
}

/// Receive the next message from this process's queue. Blocks if empty.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn recv() -> Message {
    let mut msg = Message { sender: 0, msg_type: 0, data: 0, len: 0 };
    toyos_abi::syscall::recv_msg(&mut msg as *mut Message as u64);
    msg
}
