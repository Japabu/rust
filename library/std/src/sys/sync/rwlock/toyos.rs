use core::sync::atomic::{AtomicI32, Ordering};

pub struct RwLock {
    // 0 = unlocked, positive = reader count, -1 = write-locked
    state: AtomicI32,
}

unsafe impl Send for RwLock {}
unsafe impl Sync for RwLock {}

impl RwLock {
    #[inline]
    pub const fn new() -> RwLock {
        RwLock { state: AtomicI32::new(0) }
    }

    #[inline]
    pub fn read(&self) {
        loop {
            let s = self.state.load(Ordering::Relaxed);
            if s >= 0 {
                if self.state.compare_exchange_weak(s, s + 1, Ordering::Acquire, Ordering::Relaxed).is_ok() {
                    return;
                }
            }
            core::hint::spin_loop();
        }
    }

    #[inline]
    pub fn try_read(&self) -> bool {
        let s = self.state.load(Ordering::Relaxed);
        if s >= 0 {
            self.state.compare_exchange(s, s + 1, Ordering::Acquire, Ordering::Relaxed).is_ok()
        } else {
            false
        }
    }

    #[inline]
    pub fn write(&self) {
        while self.state.compare_exchange_weak(0, -1, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
    }

    #[inline]
    pub fn try_write(&self) -> bool {
        self.state.compare_exchange(0, -1, Ordering::Acquire, Ordering::Relaxed).is_ok()
    }

    #[inline]
    pub unsafe fn read_unlock(&self) {
        self.state.fetch_sub(1, Ordering::Release);
    }

    #[inline]
    pub unsafe fn write_unlock(&self) {
        self.state.store(0, Ordering::Release);
    }

    #[inline]
    pub unsafe fn downgrade(&self) {
        self.state.store(1, Ordering::Release);
    }
}
