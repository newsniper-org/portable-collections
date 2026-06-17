//! [`HeaplessLog`] — a bare-`no_std`, no-allocator [`ScopedStack`] over a fixed
//! inline array. The one `ScopedStack` impl that compiles without `alloc`.

use portable_collection_primitives::{
    Checkpoint, Container, Pop, Push, ScopedRollback, ScopedStack, TryPush,
};

/// A bare-`no_std`, no-allocator [`ScopedStack`] over a fixed-capacity inline
/// `[Option<T>; N]` (LIFO). The only `ScopedStack` implementation that compiles
/// without `alloc`, for small **bounded** trails on heapless targets (an MCU, a
/// pre-heap kernel/bootloader stage, a const arena).
///
/// `Mark` stays [`Checkpoint`] (a `usize`), so it nests with the same scope
/// stack as `VecLog`. The capacity `N` is a hard ceiling: [`push`](Push::push)
/// **panics** on overflow (the infallible [`Push`] contract leaves no other
/// option), so prefer [`try_push`](TryPush::try_push) whenever the bound is not
/// statically guaranteed.
#[derive(Clone, Debug)]
pub struct HeaplessLog<T, const N: usize> {
    items: [Option<T>; N],
    len: usize,
}

impl<T, const N: usize> HeaplessLog<T, N> {
    /// Create an empty log.
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: core::array::from_fn(|_| None),
            len: 0,
        }
    }

    /// The fixed capacity `N`.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Whether the log is at capacity (`len == N`).
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.len == N
    }
}

impl<T, const N: usize> Default for HeaplessLog<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> Container for HeaplessLog<T, N> {
    fn clear(&mut self) {
        for slot in &mut self.items[..self.len] {
            *slot = None;
        }
        self.len = 0;
    }
    fn len(&self) -> usize {
        self.len
    }
}

impl<T, const N: usize> Push<T> for HeaplessLog<T, N> {
    /// Append `item`. **Panics** if the log is full (`len == N`); use
    /// [`try_push`](TryPush::try_push) to handle the full case as a value.
    fn push(&mut self, item: T) {
        assert!(self.len < N, "HeaplessLog overflow: capacity {N} exceeded");
        self.items[self.len] = Some(item);
        self.len += 1;
    }
}

impl<T, const N: usize> TryPush<T> for HeaplessLog<T, N> {
    /// Append `item` if there is room; return `false` (inserting nothing) when full.
    fn try_push(&mut self, item: T) -> bool {
        if self.len == N {
            return false;
        }
        self.items[self.len] = Some(item);
        self.len += 1;
        true
    }
}

impl<T, const N: usize> Pop<T> for HeaplessLog<T, N> {
    fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        self.items[self.len].take()
    }
    fn last(&self) -> Option<&T> {
        self.len.checked_sub(1).and_then(|i| self.items[i].as_ref())
    }
}

impl<T, const N: usize> ScopedRollback for HeaplessLog<T, N> {
    type Mark = Checkpoint;

    fn checkpoint(&self) -> Checkpoint {
        Checkpoint::from_len(self.len)
    }

    fn rollback_to(&mut self, mark: Checkpoint) {
        let to = mark.as_len().min(self.len);
        for slot in &mut self.items[to..self.len] {
            *slot = None;
        }
        self.len = to;
    }
}

impl<T, const N: usize> ScopedStack<T> for HeaplessLog<T, N> {
    fn drain_since(&mut self, mark: Checkpoint) -> impl Iterator<Item = T> + '_ {
        let to = mark.as_len().min(self.len);
        let mut cur = self.len; // old top
        self.len = to; // truncate logically up front
        // Yield LIFO (reverse-push): take from old_top-1 down to `to`.
        core::iter::from_fn(move || {
            if cur > to {
                cur -= 1;
                self.items[cur].take()
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::vec::Vec;

    use super::*;

    #[test]
    fn push_pop_last_len() {
        let mut log: HeaplessLog<u32, 4> = HeaplessLog::new();
        log.push(10);
        log.push(20);
        assert_eq!(log.last(), Some(&20));
        assert_eq!(Container::len(&log), 2);
        assert_eq!(log.pop(), Some(20));
        assert_eq!(log.pop(), Some(10));
        assert_eq!(log.pop(), None);
        assert!(Container::is_empty(&log));
    }

    #[test]
    fn try_push_fails_when_full_without_panic() {
        let mut log: HeaplessLog<u32, 2> = HeaplessLog::new();
        assert!(log.try_push(1));
        assert!(log.try_push(2));
        assert!(log.is_full());
        assert!(!log.try_push(3)); // full → false, nothing inserted
        assert_eq!(Container::len(&log), 2);
    }

    #[test]
    #[should_panic(expected = "overflow")]
    fn push_panics_when_full() {
        let mut log: HeaplessLog<u32, 1> = HeaplessLog::new();
        log.push(1);
        log.push(2); // infallible push has no room → panic
    }

    #[test]
    fn drain_since_yields_lifo_and_truncates() {
        let mut log: HeaplessLog<u32, 8> = HeaplessLog::new();
        log.push(1);
        let mark = ScopedRollback::checkpoint(&log);
        log.push(2);
        log.push(3);
        let drained: Vec<u32> = log.drain_since(mark).collect();
        assert_eq!(drained, [3, 2]); // LIFO
        assert_eq!(Container::len(&log), 1);
        assert_eq!(log.last(), Some(&1));
    }

    #[test]
    fn rollback_to_is_the_silent_twin() {
        let mut log: HeaplessLog<u32, 8> = HeaplessLog::new();
        log.push(1);
        let mark = ScopedRollback::checkpoint(&log);
        log.push(2);
        log.push(3);
        ScopedRollback::rollback_to(&mut log, mark);
        assert_eq!(Container::len(&log), 1);
        assert_eq!(log.last(), Some(&1));
    }

    #[test]
    fn drain_since_overshoot_is_empty_noop() {
        let mut log: HeaplessLog<u32, 4> = HeaplessLog::new();
        log.push(1);
        let big = Checkpoint::from_len(99);
        let drained: Vec<u32> = log.drain_since(big).collect();
        assert!(drained.is_empty());
        assert_eq!(Container::len(&log), 1);
    }
}
