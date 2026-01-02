use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

// --- SPINLOCK IMPLEMENTATION ---

pub struct Spinlock<T> {
    lock: AtomicBool,
    data: UnsafeCell<T>,
}

// Báo cho Rust biết Spinlock an toàn để chia sẻ giữa các luồng
unsafe impl<T: Send> Sync for Spinlock<T> {}

impl<T> Spinlock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&'_ self) -> SpinlockGuard<'_, T> {
        // Busy-wait (xoay vòng) cho đến khi lock được mở (false)
        // compare_exchange hoặc swap đều được. Ở đây dùng loop đơn giản:
        while self
            .lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            // Gợi ý cho CPU biết ta đang trong vòng lặp spin (để tiết kiệm điện/tối ưu pipeline)
            core::hint::spin_loop();
        }

        SpinlockGuard {
            lock: &self.lock,
            data: unsafe { &mut *self.data.get() },
        }
    }
}

// Guard để tự động unlock khi ra khỏi scope (RAII)
pub struct SpinlockGuard<'a, T> {
    lock: &'a AtomicBool,
    data: &'a mut T,
}

impl<'a, T> Deref for SpinlockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.data
    }
}

impl<'a, T> DerefMut for SpinlockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data
    }
}

impl<'a, T> Drop for SpinlockGuard<'a, T> {
    fn drop(&mut self) {
        // Mở khóa (về false)
        self.lock.store(false, Ordering::Release);
    }
}
