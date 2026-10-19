//! One process-wide lock for pointer injection and text insertion.

use std::cell::Cell;

use parking_lot::{Mutex, MutexGuard};

static IO_LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    static DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// True when this thread already holds [`with_io_serial`].
pub fn io_lock_held() -> bool {
    DEPTH.with(|d| d.get() > 0)
}

pub struct IoGuard {
    _guard: MutexGuard<'static, ()>,
}

impl IoGuard {
    pub fn enter() -> Self {
        let guard = IO_LOCK.lock();
        DEPTH.with(|d| d.set(d.get() + 1));
        Self { _guard: guard }
    }
}

impl Drop for IoGuard {
    fn drop(&mut self) {
        DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Run `f` while pointer moves and `TextInserter` are excluded.
///
/// Not re-entrant. Dictation edges must call this only around the OS write,
/// never around code that waits on another holder of the same lock.
pub fn with_io_serial<R>(f: impl FnOnce() -> R) -> R {
    let _guard = IoGuard::enter();
    f()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    use super::*;

    #[test]
    fn holders_do_not_overlap() {
        let depth = AtomicUsize::new(0);
        let max_depth = AtomicUsize::new(0);
        thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    for _ in 0..40 {
                        with_io_serial(|| {
                            let now = depth.fetch_add(1, Ordering::SeqCst) + 1;
                            max_depth.fetch_max(now, Ordering::SeqCst);
                            thread::sleep(Duration::from_micros(30));
                            depth.fetch_sub(1, Ordering::SeqCst);
                        });
                    }
                });
            }
        });
        assert_eq!(max_depth.load(Ordering::SeqCst), 1);
        assert!(!io_lock_held());
    }
}
