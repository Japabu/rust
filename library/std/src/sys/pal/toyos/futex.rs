use crate::sync::atomic::Atomic;
use crate::time::Duration;

pub type Futex = Atomic<Primitive>;
pub type Primitive = u32;

pub type SmallFutex = Atomic<SmallPrimitive>;
pub type SmallPrimitive = u32;

pub fn futex_wait(futex: &Atomic<u32>, expected: u32, timeout: Option<Duration>) -> bool {
    let timeout_ns = timeout
        .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX));

    let r = toyos_abi::syscall::futex_wait(futex.as_ptr(), expected, timeout_ns);
    r != 1 // 1 = timed out
}

#[inline]
pub fn futex_wake(futex: &Atomic<u32>) -> bool {
    toyos_abi::syscall::futex_wake(futex.as_ptr(), 1) > 0
}

#[inline]
pub fn futex_wake_all(futex: &Atomic<u32>) {
    toyos_abi::syscall::futex_wake(futex.as_ptr(), u32::MAX);
}
