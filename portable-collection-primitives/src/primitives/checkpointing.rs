//! Shared scope-checkpoint/rollback vocabulary implemented by every
//! `portable-collections` member.
//!
//! Two items live here: the opaque [`Checkpoint`] mark and the
//! [`ScopedRollback`] contract trait. Both are **heap-free** — they touch
//! neither `alloc` nor `std` — so they are present even in a bare `no_std`
//! build, letting a future heapless collection share the exact same rollback
//! vocabulary as the `alloc`-backed ones.

/// An opaque mark of a collection's size at one point in time, taken by
/// [`ScopedRollback::checkpoint`] and consumed by
/// [`ScopedRollback::rollback_to`].
///
/// It gives the scope-rollback mark a *name and a type* instead of a bare
/// `usize`, so a mark is not silently confused with a length, a slice index, or
/// an id. This is a *soft* guard against accidental mix-ups, not a hard one:
/// [`from_len`](Self::from_len) is public, so a mark built for one collection can
/// still be handed to another's [`rollback_to`](ScopedRollback::rollback_to) —
/// but a too-large mark is a no-op (overshoot, contract law 4) and a too-small
/// one only truncates, exactly as the already-public inherent `usize` API
/// allows. The type removes the *accidental* mix-up, not deliberate misuse.
///
/// The inner count is deliberately private and there is **no**
/// `From<usize>`/`Into<usize>`/`Deref`: marks are opaque tokens, not integers to
/// do arithmetic on. The narrow [`from_len`](Self::from_len) /
/// [`as_len`](Self::as_len) bridge is for the collection that mints and consumes
/// them, named so the `len <-> mark` direction is explicit at every call site.
///
/// Ordering is by captured size, so an outer (earlier) scope compares `<` an
/// inner (later) one — handy for nested-scope assertions. `Copy`, so it drops
/// into a consumer's per-scope state struct with zero friction.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Checkpoint(usize);

impl Checkpoint {
    /// The origin mark, equal to the checkpoint of an empty collection.
    /// `rollback_to(Checkpoint::ORIGIN)` therefore rolls everything back.
    pub const ORIGIN: Checkpoint = Checkpoint(0);

    /// Mint a mark from a collection's current logical length. Intended for the
    /// collection implementing [`ScopedRollback`]; callers receive marks from
    /// [`checkpoint`](ScopedRollback::checkpoint) rather than constructing them.
    #[must_use]
    pub const fn from_len(len: usize) -> Self {
        Checkpoint(len)
    }

    /// The length this mark was captured at — for a backend to compare against
    /// its current size when rolling back, and for diagnostics. This is the
    /// *only* way back out to a `usize`, by design (no `From`/`Deref`).
    #[must_use]
    pub const fn as_len(self) -> usize {
        self.0
    }
}

/// The scope checkpoint/rollback contract that is the reason
/// `portable-collections` exists.
///
/// [`checkpoint`](Self::checkpoint) records a mark; [`rollback_to`](Self::rollback_to)
/// atomically rolls **every** backing store of the collection back to that mark,
/// so a multi-store desync — the SMT interner `push`/`pop` bug this workspace was
/// extracted to kill — cannot be written.
///
/// # Contract (the load-bearing part — a signature alone cannot enforce it)
///
/// For any value `c: Self`, an implementor MUST guarantee:
///
/// 1. **Round-trip identity.** `let m = c.checkpoint(); c.rollback_to(m);`
///    leaves `c` observably unchanged through *every* accessor.
/// 2. **Atomic across all stores.** After `rollback_to(m)`, nothing added since
///    `m` is observable in ANY direction or index of the collection — forward
///    map, reverse map, order log, secondary structures, all of it. This is the
///    desync-prevention property.
/// 3. **Reverse-order removal.** Entries are dropped last-in-first-out, matching
///    scope nesting.
/// 4. **Overshoot is a no-op.** A mark at or beyond the current size leaves the
///    collection unchanged.
/// 5. **Idempotent.** `c.rollback_to(m); c.rollback_to(m);` equals a single
///    `c.rollback_to(m)`.
///
/// These laws are a documented obligation; the workspace tests them per type.
pub trait ScopedRollback {
    /// The opaque mark type. A backend backed by a dense count uses
    /// [`Checkpoint`] (write `type Mark = Checkpoint;`); a backend whose natural
    /// mark is not a length — e.g. a generation id for a persistent or radix
    /// backend — may use its own `Copy` mark instead.
    ///
    /// An associated-type *default* of `Checkpoint` would need nightly
    /// `associated_type_defaults`; with the workspace's stable `rust-version`
    /// each impl writes the one-line `type Mark = Checkpoint;`.
    type Mark: Copy;

    /// Capture a mark of the current state for a later
    /// [`rollback_to`](Self::rollback_to). Cheap and side-effect-free.
    #[must_use]
    fn checkpoint(&self) -> Self::Mark;

    /// Atomically discard everything captured after `mark`, across all backing
    /// stores, per the contract above. Overshooting marks are no-ops (law 4).
    fn rollback_to(&mut self, mark: Self::Mark);

    /// Discard every entry.
    ///
    /// Not derivable generically — the origin mark is `Mark`-specific — so it is
    /// a required method; for `Mark = Checkpoint` an impl is one line
    /// (`self.rollback_to(Checkpoint::ORIGIN)`), but a backend may clear more
    /// directly.
    fn clear(&mut self);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_is_zero_and_is_the_default() {
        assert_eq!(Checkpoint::ORIGIN.as_len(), 0);
        assert_eq!(Checkpoint::default(), Checkpoint::ORIGIN);
    }

    #[test]
    fn from_len_as_len_round_trip() {
        for n in [0usize, 1, 7, 1000] {
            assert_eq!(Checkpoint::from_len(n).as_len(), n);
        }
    }

    #[test]
    fn ordering_follows_captured_len() {
        // An outer (earlier, smaller) scope compares `<` an inner (later) one.
        assert!(Checkpoint::from_len(2) < Checkpoint::from_len(5));
        assert_eq!(Checkpoint::from_len(4), Checkpoint::from_len(4));
        assert!(Checkpoint::ORIGIN <= Checkpoint::from_len(0));
    }
}
