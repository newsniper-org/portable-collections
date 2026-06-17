//! Scoped append-logs — [`ScopedStack`] (LIFO) and [`ScopedQueue`] (FIFO): the
//! undo-ledger companions of [`Bimap`](super::Bimap) for the *primary-store ↔
//! undo-ledger* desync (a clause DB and its per-scope drop list; an SMT trail).
//!
//! Only the traits live here; concrete implementations (`VecScopedStack`, the
//! bare-no_std `ArrayScopedStack`, `DequeScopedQueue`, …) live in the
//! `portable-queues` crate.

use super::{Pop, Pull, Push};

/// A **LIFO** append-only sequence with scope checkpoint/rollback — the
/// stack-flavored undo ledger (an SMT trail / a clause-DB per-scope "what to drop
/// on pop" list). Append with [`Push`], inspect/remove the most-recent with
/// [`Pop`]; checkpoint/rollback come from
/// [`ScopedRollback`](super::ScopedRollback) (with `len`/`clear` via `Container`).
/// This trait adds the *yielding* unwind.
///
/// Every element is recorded as it is pushed, and one
/// [`drain_since`](Self::drain_since) discards exactly the suffix added after a
/// mark, in lock-step with a scope stack — so a ledger that must unwind with its
/// scope cannot fall out of sync.
pub trait ScopedStack<T>: super::ScopedRollback + Push<T> + Pop<T> {
    /// Discard everything pushed after `mark` and **yield it in reverse-push
    /// (LIFO)** order (most-recent first), so the caller can run per-item cleanup
    /// as the scope unwinds (e.g. drop each clause id from its clause DB + watch
    /// lists). Overshoot → empty (no-op). The yielding companion of
    /// [`rollback_to`](super::ScopedRollback::rollback_to), which discards silently.
    fn drain_since(
        &mut self,
        mark: <Self as super::ScopedRollback>::Mark,
    ) -> impl Iterator<Item = T> + '_;
}

/// A **FIFO** append-only sequence with scope checkpoint/rollback — the
/// queue-flavored sibling of [`ScopedStack`]. Append with [`Push`] (at the back),
/// inspect/remove the oldest with [`Pull`] (at the front); checkpoint/rollback
/// come from [`ScopedRollback`](super::ScopedRollback).
pub trait ScopedQueue<T>: super::ScopedRollback + Push<T> + Pull<T> {
    /// Discard everything pushed after `mark` and **yield it in push (FIFO)**
    /// order (oldest of the rolled-back suffix first). Overshoot → empty (no-op).
    /// The yielding companion of
    /// [`rollback_to`](super::ScopedRollback::rollback_to), which discards silently.
    fn drain_since(
        &mut self,
        mark: <Self as super::ScopedRollback>::Mark,
    ) -> impl Iterator<Item = T> + '_;
}
