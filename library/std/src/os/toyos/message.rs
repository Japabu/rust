//! Type-safe kernel message queue API for inter-process communication.
//!
//! Messages carry a `msg_type` tag and an optional typed payload. The kernel
//! copies payload bytes between address spaces — sender and receiver don't
//! need to share memory.

/// A message passed between processes via the kernel message queue.
///
/// Payload is owned as a heap-allocated buffer. Use [`Message::new`] to create
/// a message with a typed payload, and [`Message::take_payload`] to extract it
/// on the receiving side.
#[repr(C)]
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub struct Message {
    sender: u32,
    msg_type: u32,
    payload: Vec<u8>,
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl Message {
    /// Create a message with a typed payload.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn new<T>(msg_type: u32, payload: T) -> Self {
        let len = core::mem::size_of::<T>();
        let bytes = unsafe {
            core::slice::from_raw_parts(&payload as *const T as *const u8, len)
        }.to_vec();
        core::mem::forget(payload);
        Self { sender: 0, msg_type, payload: bytes }
    }

    /// Create a message with no payload.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn signal(msg_type: u32) -> Self {
        Self { sender: 0, msg_type, payload: Vec::new() }
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
        Self { sender: 0, msg_type, payload: bytes.to_vec() }
    }

    /// Extract the payload as raw bytes, consuming the message.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn take_bytes(self) -> Vec<u8> {
        self.payload
    }

    /// Extract the typed payload, consuming the message.
    /// Panics if `size_of::<T>()` doesn't match the payload size.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn take_payload<T>(self) -> T {
        let expected = core::mem::size_of::<T>();
        assert_eq!(
            self.payload.len(), expected,
            "message payload size mismatch: expected {expected}, got {}",
            self.payload.len(),
        );
        let value = unsafe { core::ptr::read_unaligned(self.payload.as_ptr() as *const T) };
        value
    }

    /// Payload as a byte slice.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub fn bytes(&self) -> &[u8] {
        &self.payload
    }
}

/// Send a message to another process. Returns `Err` if the target process
/// no longer exists.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn send(target_pid: u32, msg: Message) -> Result<(), SendError> {
    let raw = toyos_abi::message::RawMessage {
        sender: 0,
        msg_type: msg.msg_type,
        data: msg.payload.as_ptr() as u64,
        len: msg.payload.len() as u64,
    };
    let result = unsafe { toyos_abi::syscall::send_msg(target_pid as u64, &raw as *const _ as u64) };
    if result == 0 { Ok(()) } else { Err(SendError { target_pid }) }
}

/// Error returned when a message cannot be delivered.
#[stable(feature = "toyos_ext", since = "1.0.0")]
#[derive(Debug)]
pub struct SendError {
    /// The PID that was unreachable.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    pub target_pid: u32,
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl core::fmt::Display for SendError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "failed to send message to pid {}", self.target_pid)
    }
}

/// Default receive buffer size.
const DEFAULT_BUF_SIZE: usize = 4096;

/// Receive the next message from this process's queue. Blocks if empty.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn recv() -> Message {
    let mut buf = vec![0u8; DEFAULT_BUF_SIZE];
    let (raw, actual_len) = toyos_abi::message::recv_into(&mut buf);
    buf.truncate(actual_len.min(DEFAULT_BUF_SIZE));
    Message {
        sender: raw.sender,
        msg_type: raw.msg_type,
        payload: buf,
    }
}
