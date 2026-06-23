//! The byte-keyed **ordered-map** vocabulary: ordered `range` scans
//! (lazy/borrowed, eager/owned, and a zero-alloc visitor) and O(1) `snapshot`s.
//!
//! Built on the [`MapReadShim`]/[`MapRefKeyInsertShim`] facade for byte-slice
//! (`[u8]`) keys. It lives in `primitives` so any ordered backend (radix,
//! B+tree, …) shares the same contract. It needs `alloc` (its iterators yield
//! owned `Vec<u8>` keys — a radix key is the traversal path, so the key is
//! materialized even when the value is borrowed), so the module is gated to the
//! alloc/std tiers.

use super::{Container, MapReadShim, MapRefKeyInsertShim};

ifstd!({
    use std::vec::Vec;
} else {
    ifalloc!({
        extern crate alloc;
        use alloc::vec::Vec;
    });
});

/// An ordered map keyed by byte slices (`[u8]`), scannable in ascending
/// (lexicographic) key order.
///
/// Three range accessors, cheapest-borrow first:
/// - [`for_each_range`](OrderedMap::for_each_range) — a **zero-allocation**
///   visitor `f(&[u8], &V)` (builds no key `Vec`).
/// - [`range_ref`](OrderedMap::range_ref) — a **lazy iterator** of
///   `(owned key, &value)` (borrows values; materializes keys on demand).
/// - [`range`](OrderedMap::range) — a lazy iterator of **owned** `(key, value)`
///   (clones each value); defaults to cloning `range_ref`.
pub trait OrderedMap<V>: MapReadShim<[u8], V> + MapRefKeyInsertShim<[u8], V> + Container {
    /// **Lazy** ordered scan over the inclusive byte-range `[lo, hi]`, ascending
    /// by key, yielding `(owned key, &value)` on demand — values are borrowed
    /// (no per-item value clone). The key is materialized because a radix key is
    /// the traversal path.
    fn range_ref<'a>(&'a self, lo: &[u8], hi: &[u8]) -> impl Iterator<Item = (Vec<u8>, &'a V)>
    where
        V: 'a;

    /// **Zero-allocation** ordered visitor: call `f(key, value)` for each entry
    /// in `[lo, hi]` in ascending key order, borrowing both — no key `Vec` is
    /// built. The cheapest scan when an iterator is not needed.
    fn for_each_range<F: FnMut(&[u8], &V)>(&self, lo: &[u8], hi: &[u8], f: F);

    /// Lazy ordered scan yielding **owned** `(key, value)` pairs (clones each
    /// value). Defaults to cloning [`range_ref`](Self::range_ref).
    fn range<'a>(&'a self, lo: &[u8], hi: &[u8]) -> impl Iterator<Item = (Vec<u8>, V)>
    where
        V: Clone + 'a,
    {
        self.range_ref(lo, hi).map(|(k, v)| (k, v.clone()))
    }
}

/// A map supporting **O(1) snapshots** (an immutable, isolated point-in-time
/// view).
///
/// ## Contract (the snapshot laws)
///
/// For a copy-on-write implementor, [`snapshot`](SnapshotMap::snapshot) holds:
///
/// 1. **O(1)** — taken in constant time (an `Arc`-clone of the root),
///    independent of [`len`](Container::len).
/// 2. **Isolation** — the result is an independent [`OrderedMap`]; mutations to
///    the live map after the snapshot never change it.
/// 3. **Point-in-time** — it observes exactly the key/value set live at the
///    call, and nothing written later.
/// 4. **Structure sharing** — it shares unmodified structure with the live map;
///    later writes path-copy only the touched path (so isolation is free).
pub trait SnapshotMap<V>: OrderedMap<V> {
    /// The snapshot type (itself an [`OrderedMap`] for reads).
    type Snapshot: OrderedMap<V>;

    /// Take a snapshot — see the [trait contract](SnapshotMap).
    #[must_use]
    fn snapshot(&self) -> Self::Snapshot;
}
