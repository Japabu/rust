use core::sync::atomic::{AtomicU8, Ordering};

use crate::sync as public;
use crate::sync::once::OnceExclusiveState;

const INCOMPLETE: u8 = 0;
const POISONED: u8 = 1;
const RUNNING: u8 = 2;
const COMPLETE: u8 = 3;

pub struct Once {
    state: AtomicU8,
}

pub struct OnceState {
    poisoned: bool,
    set_state_to: AtomicU8,
}

struct CompletionGuard<'a> {
    state: &'a AtomicU8,
    set_state_on_drop_to: u8,
}

impl<'a> Drop for CompletionGuard<'a> {
    fn drop(&mut self) {
        self.state.store(self.set_state_on_drop_to, Ordering::Release);
    }
}

unsafe impl Sync for Once {}

impl Once {
    #[inline]
    pub const fn new() -> Once {
        Once { state: AtomicU8::new(INCOMPLETE) }
    }

    #[inline]
    pub fn is_completed(&self) -> bool {
        self.state.load(Ordering::Acquire) == COMPLETE
    }

    #[inline]
    pub(crate) fn state(&mut self) -> OnceExclusiveState {
        match *self.state.get_mut() {
            INCOMPLETE => OnceExclusiveState::Incomplete,
            POISONED => OnceExclusiveState::Poisoned,
            COMPLETE => OnceExclusiveState::Complete,
            _ => unreachable!("invalid Once state"),
        }
    }

    #[inline]
    pub(crate) fn set_state(&mut self, new_state: OnceExclusiveState) {
        *self.state.get_mut() = match new_state {
            OnceExclusiveState::Incomplete => INCOMPLETE,
            OnceExclusiveState::Poisoned => POISONED,
            OnceExclusiveState::Complete => COMPLETE,
        };
    }

    #[cold]
    #[track_caller]
    pub fn wait(&self, _ignore_poisoning: bool) {
        while self.state.load(Ordering::Acquire) == RUNNING {
            core::hint::spin_loop();
        }
        if !_ignore_poisoning && self.state.load(Ordering::Acquire) == POISONED {
            panic!("Once instance has previously been poisoned");
        }
    }

    #[cold]
    #[track_caller]
    pub fn call(&self, ignore_poisoning: bool, f: &mut impl FnMut(&public::OnceState)) {
        loop {
            let state = self.state.load(Ordering::Acquire);
            match state {
                POISONED if !ignore_poisoning => {
                    panic!("Once instance has previously been poisoned");
                }
                INCOMPLETE | POISONED => {
                    if self.state.compare_exchange(state, RUNNING, Ordering::Acquire, Ordering::Relaxed).is_err() {
                        continue;
                    }
                    let mut guard = CompletionGuard {
                        state: &self.state,
                        set_state_on_drop_to: POISONED,
                    };
                    let f_state = public::OnceState {
                        inner: OnceState {
                            poisoned: state == POISONED,
                            set_state_to: AtomicU8::new(COMPLETE),
                        },
                    };
                    f(&f_state);
                    guard.set_state_on_drop_to = f_state.inner.set_state_to.load(Ordering::Relaxed);
                    return;
                }
                RUNNING => {
                    core::hint::spin_loop();
                }
                COMPLETE => return,
                _ => unreachable!(),
            }
        }
    }
}

impl OnceState {
    #[inline]
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    #[inline]
    pub fn poison(&self) {
        self.set_state_to.store(POISONED, Ordering::Relaxed);
    }
}
