//! The minimal collection vocabulary — clear it, ask its size — shared as a
//! supertrait by [`ScopedRollback`](super::ScopedRollback), [`Map`](super::Map),
//! and [`Set`](super::Set).
//!
//! Heap-free (it touches neither `alloc` nor `std`), so it is present in every
//! feature tier — which is what lets the always-present `ScopedRollback` use it
//! as a supertrait.

/// The minimal state any collection in this workspace exposes: the ability to
/// empty it and to ask its size.
pub trait Container {

    /// The number of entries currently held.
    #[must_use]
    fn len(&self) -> usize;

    /// Whether the collection holds no entries.
    #[must_use]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub trait Clearable: Container {
    /// Remove all entries.
    fn clear(&mut self);
}