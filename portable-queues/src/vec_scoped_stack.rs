//! [`VecScopedStack`] — the canonical `Vec`-backed [`ScopedStack`] implementation.

use portable_collection_primitives::{ifstd, ifstdoralloc};

ifstd!({
    use std::vec::Vec;
} else {
    use portable_collection_primitives::ifalloc;
    ifalloc!({
        extern crate alloc;
        use alloc::vec::Vec;
    });
});

ifstdoralloc!({
    use portable_collection_primitives::{Checkpoint, Container, ScopedRollback, ScopedStack, Push, Pop};

    /// A `Vec`-backed append-only log with scope checkpoint/rollback — the
    /// canonical [`ScopedStack`] implementation. `push` appends; `checkpoint` marks
    /// the current length; `rollback_to` / `drain_since` cut the suffix back to a
    /// mark (silently / yielding LIFO, respectively).
    ///
    /// ```
    /// use portable_queues::VecScopedStack;
    /// use portable_collection_primitives::{ScopedStack, ScopedRollback, Container, Push};
    ///
    /// let mut log: VecScopedStack<u32> = VecScopedStack::new();
    /// log.push(1);
    /// let mark = ScopedRollback::checkpoint(&log);   // remember the scope start
    /// log.push(2);
    /// log.push(3);
    /// // Leave the scope, yielding the popped items LIFO for per-item cleanup:
    /// let undone: Vec<u32> = log.drain_since(mark).collect();
    /// assert_eq!(undone, [3, 2]);
    /// assert_eq!(Container::len(&log), 1);
    /// ```
    #[derive(Clone, Debug)]
    pub struct VecScopedStack<T> {
        items: Vec<T>,
    }

    impl<T> VecScopedStack<T> {
        /// Create an empty log.
        #[must_use]
        pub const fn new() -> Self {
            Self { items: Vec::new() }
        }

        /// The logged items in push order.
        #[must_use]
        pub fn as_slice(&self) -> &[T] {
            &self.items
        }

        /// Iterate the logged items in push order.
        pub fn iter(&self) -> impl Iterator<Item = &T> {
            self.items.iter()
        }
    }

    impl<T> Default for VecScopedStack<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<T> Container for VecScopedStack<T> {
        fn clear(&mut self) {
            self.items.clear();
        }
        fn len(&self) -> usize {
            self.items.len()
        }
    }

    impl<T> ScopedRollback for VecScopedStack<T> {
        type Mark = Checkpoint;

        fn checkpoint(&self) -> Checkpoint {
            Checkpoint::from_len(self.items.len())
        }

        fn rollback_to(&mut self, mark: Checkpoint) {
            self.items.truncate(mark.as_len());
        }
    }

    impl<T> Push<T> for VecScopedStack<T> {
        fn push(&mut self, item: T) {
            self.items.push(item);
        }
    }

    impl<T> Pop<T> for VecScopedStack<T> {
        fn pop(&mut self) -> Option<T> {
            self.items.pop()
        }

        fn last(&self) -> Option<&T> {
            self.items.last()
        }
    }

    impl<T> ScopedStack<T> for VecScopedStack<T> {
        fn drain_since(&mut self, mark: Checkpoint) -> impl Iterator<Item = T> + '_ {
            // Overshoot (a mark at/beyond the current len) → empty drain (no-op).
            let from = mark.as_len().min(self.items.len());
            // `drain` yields front→back; reverse it for LIFO (reverse-push) order.
            self.items.drain(from..).rev()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn push_last_len() {
            let mut log: VecScopedStack<u32> = VecScopedStack::new();
            log.push(10);
            log.push(20);
            assert_eq!(log.last(), Some(&20));
            assert_eq!(Container::len(&log), 2);
            assert!(!Container::is_empty(&log));
        }

        #[test]
        fn pop_removes_last_lifo() {
            let mut log: VecScopedStack<u32> = VecScopedStack::new();
            log.push(1);
            log.push(2);
            assert_eq!(log.pop(), Some(2));
            assert_eq!(log.last(), Some(&1));
            assert_eq!(log.pop(), Some(1));
            assert_eq!(log.pop(), None);
        }

        #[test]
        fn drain_since_yields_lifo_and_truncates() {
            let mut log: VecScopedStack<u32> = VecScopedStack::new();
            log.push(1);
            let mark = ScopedRollback::checkpoint(&log);
            log.push(2);
            log.push(3);
            let drained: Vec<u32> = log.drain_since(mark).collect();
            assert_eq!(drained, [3, 2]); // LIFO: reverse-push
            assert_eq!(Container::len(&log), 1);
            assert_eq!(log.last(), Some(&1));
        }

        #[test]
        fn drain_since_overshoot_is_empty_noop() {
            let mut log: VecScopedStack<u32> = VecScopedStack::new();
            log.push(1);
            let big = Checkpoint::from_len(99);
            let drained: Vec<u32> = log.drain_since(big).collect();
            assert!(drained.is_empty());
            assert_eq!(Container::len(&log), 1);
        }

        #[test]
        fn rollback_to_is_the_silent_twin_of_drain_since() {
            let mut log: VecScopedStack<u32> = VecScopedStack::new();
            log.push(1);
            let mark = ScopedRollback::checkpoint(&log);
            log.push(2);
            log.push(3);
            ScopedRollback::rollback_to(&mut log, mark);
            assert_eq!(Container::len(&log), 1);
            assert_eq!(log.last(), Some(&1));
        }

        #[test]
        fn round_trip_identity() {
            let mut log: VecScopedStack<u32> = VecScopedStack::new();
            log.push(1);
            log.push(2);
            let m = ScopedRollback::checkpoint(&log);
            ScopedRollback::rollback_to(&mut log, m);
            assert_eq!(log.as_slice(), &[1, 2]);
        }
    }
});
