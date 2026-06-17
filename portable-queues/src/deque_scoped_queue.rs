//! [`DequeScopedQueue`] — the canonical `VecDeque`-backed [`ScopedQueue`]
//! implementation.

use portable_collection_primitives::{ifstd, ifstdoralloc};

ifstd!({
    use std::collections::VecDeque;
} else {
    use portable_collection_primitives::ifalloc;
    ifalloc!({
        extern crate alloc;
        use alloc::collections::VecDeque;
    });
});

ifstdoralloc!({
    use portable_collection_primitives::{Checkpoint, Container, Pull, Push, ScopedQueue, ScopedRollback};

    /// A `VecDeque`-backed **FIFO** append-log with scope checkpoint/rollback — the
    /// queue-flavored sibling of `VecScopedStack`. [`push`](Push::push) appends at
    /// the back; [`pull`](Pull::pull) consumes at the front; `drain_since` peels the
    /// post-mark suffix off the back in FIFO (push) order.
    ///
    /// **Scope semantics (push-scoped).** Because push (back) and pull (front)
    /// touch opposite ends, only the *push* side is scope-rollback-able: the
    /// `Mark` is an absolute push counter that `pull` does not decrement, and
    /// `rollback_to` / `drain_since` undo exactly the pushes made after the mark.
    /// `pull` is a FIFO consume that is not itself rolled back — so the
    /// `ScopedRollback` round-trip law holds for a scope that only *pushes* (the
    /// intended "build inside the scope, drain outside it" pattern); pulling the
    /// scope's own freshly-pushed items inside the same scope is outside that
    /// contract.
    #[derive(Clone, Debug)]
    pub struct DequeScopedQueue<T> {
        items: VecDeque<T>,
        /// Total items ever pushed — the checkpoint mark space. `pull` does NOT
        /// decrement it (pull is a front-consume, not a push-rollback).
        gen: usize,
    }

    impl<T> DequeScopedQueue<T> {
        /// Create an empty queue.
        #[must_use]
        pub const fn new() -> Self {
            Self { items: VecDeque::new(), gen: 0 }
        }

        /// Iterate the live items in FIFO (front-to-back) order.
        pub fn iter(&self) -> impl Iterator<Item = &T> {
            self.items.iter()
        }
    }

    impl<T> Default for DequeScopedQueue<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<T> Container for DequeScopedQueue<T> {
        fn clear(&mut self) {
            self.items.clear();
            self.gen = 0;
        }
        fn len(&self) -> usize {
            self.items.len()
        }
    }

    impl<T> Push<T> for DequeScopedQueue<T> {
        fn push(&mut self, item: T) {
            self.items.push_back(item);
            self.gen += 1;
        }
    }

    impl<T> Pull<T> for DequeScopedQueue<T> {
        fn pull(&mut self) -> Option<T> {
            self.items.pop_front()
        }
        fn first(&self) -> Option<&T> {
            self.items.front()
        }
    }

    impl<T> ScopedRollback for DequeScopedQueue<T> {
        type Mark = Checkpoint;

        fn checkpoint(&self) -> Checkpoint {
            Checkpoint::from_len(self.gen)
        }

        fn rollback_to(&mut self, mark: Checkpoint) {
            let target = mark.as_len();
            // Drop pushes made after the mark, from the back. Overshoot (target
            // at/beyond gen) is a no-op.
            while self.gen > target {
                if self.items.pop_back().is_some() {
                    self.gen -= 1;
                } else {
                    // Back is empty before reaching target → the scope's pushes
                    // were already pulled; resync the counter and stop.
                    self.gen = target;
                    break;
                }
            }
        }
    }

    impl<T> ScopedQueue<T> for DequeScopedQueue<T> {
        fn drain_since(&mut self, mark: Checkpoint) -> impl Iterator<Item = T> + '_ {
            let target = mark.as_len();
            let n = self.gen.saturating_sub(target).min(self.items.len());
            self.gen -= n;
            let start = self.items.len() - n;
            // `VecDeque::drain` yields front-to-back, i.e. the scope suffix in
            // push (FIFO) order.
            self.items.drain(start..)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn push_pull_fifo() {
            let mut q: DequeScopedQueue<u32> = DequeScopedQueue::new();
            q.push(1);
            q.push(2);
            q.push(3);
            assert_eq!(q.first(), Some(&1));
            assert_eq!(q.pull(), Some(1)); // FIFO: oldest first
            assert_eq!(q.pull(), Some(2));
            assert_eq!(Container::len(&q), 1);
            assert_eq!(q.pull(), Some(3));
            assert_eq!(q.pull(), None);
        }

        #[test]
        fn drain_since_yields_fifo_and_rolls_back_pushes() {
            let mut q: DequeScopedQueue<u32> = DequeScopedQueue::new();
            q.push(1);
            let mark = ScopedRollback::checkpoint(&q);
            q.push(2);
            q.push(3);
            // Scope suffix yielded in push (FIFO) order, then removed.
            {
                let mut drained = q.drain_since(mark);
                assert_eq!(drained.next(), Some(2));
                assert_eq!(drained.next(), Some(3));
                assert_eq!(drained.next(), None);
            }
            assert_eq!(Container::len(&q), 1);
            assert_eq!(q.first(), Some(&1));
        }

        #[test]
        fn rollback_to_is_the_silent_twin_for_pushes() {
            let mut q: DequeScopedQueue<u32> = DequeScopedQueue::new();
            q.push(1);
            let mark = ScopedRollback::checkpoint(&q);
            q.push(2);
            q.push(3);
            ScopedRollback::rollback_to(&mut q, mark);
            assert_eq!(Container::len(&q), 1);
            assert_eq!(q.first(), Some(&1));
        }

        #[test]
        fn rollback_overshoot_is_noop() {
            let mut q: DequeScopedQueue<u32> = DequeScopedQueue::new();
            q.push(1);
            let big = Checkpoint::from_len(99);
            ScopedRollback::rollback_to(&mut q, big);
            assert_eq!(Container::len(&q), 1);
        }

        #[test]
        fn pull_does_not_break_a_later_rollback() {
            // The "build inside, drain outside" pattern: push in a scope, pull the
            // committed prefix, then roll the scope's pushes back.
            let mut q: DequeScopedQueue<u32> = DequeScopedQueue::new();
            q.push(10); // committed (pre-scope)
            let mark = ScopedRollback::checkpoint(&q);
            q.push(20);
            q.push(30);
            assert_eq!(q.pull(), Some(10)); // consume the committed prefix
            ScopedRollback::rollback_to(&mut q, mark); // undo the scope's pushes
            assert_eq!(Container::len(&q), 0);
            assert_eq!(q.first(), None);
        }
    }
});
