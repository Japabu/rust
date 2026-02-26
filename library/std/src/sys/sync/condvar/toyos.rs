use crate::sys::sync::Mutex;
use crate::time::Duration;

pub struct Condvar {}

impl Condvar {
    #[inline]
    pub const fn new() -> Condvar {
        Condvar {}
    }

    #[inline]
    pub fn notify_one(&self) {}

    #[inline]
    pub fn notify_all(&self) {}

    pub unsafe fn wait(&self, mutex: &Mutex) {
        // Spin-based: release the mutex, yield, re-acquire.
        // The caller re-checks its condition in a loop.
        unsafe { mutex.unlock(); }
        core::hint::spin_loop();
        mutex.lock();
    }

    pub unsafe fn wait_timeout(&self, mutex: &Mutex, _dur: Duration) -> bool {
        // Spin once and return false (timeout).
        unsafe { mutex.unlock(); }
        core::hint::spin_loop();
        mutex.lock();
        false
    }
}
