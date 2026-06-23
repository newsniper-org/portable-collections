//! The byte-keyed ordered-map vocabulary: `insert / lookup / range / snapshot`.
//!
//! These are **crate-local** role traits: the radix maps are keyed by `&[u8]`,
//! so they do not fit the generic `MapShim<K, Q, V>` facade in
//! `portable-collection-primitives`. They are written in that crate's house
//! style — a documented contract per trait — so they could be promoted into the
//! shared facade later with minimal churn.

use portable_collection_primitives::ifstd;

ifstd!({
    use std::vec::Vec;
} else {
    use portable_collection_primitives::ifalloc;
    ifalloc!({
        extern crate alloc;
        use alloc::vec::Vec;
    });
});

/// An ordered map keyed by byte slices.
pub trait OrderedMap<V> {
    /// Insert / overwrite `key -> value`; returns the previous value if any.
    fn insert(&mut self, key: &[u8], value: V) -> Option<V>;

    /// Point lookup.
    #[must_use]
    fn get(&self, key: &[u8]) -> Option<&V>;

    /// Ordered scan over the inclusive byte-range `[lo, hi]`, ascending by key.
    #[must_use]
    fn range(&self, lo: &[u8], hi: &[u8]) -> Vec<(Vec<u8>, V)>
    where
        V: Clone;

    /// Number of stored keys.
    #[must_use]
    fn len(&self) -> usize;

    /// Whether the map holds no keys.
    #[must_use]
    fn is_empty(&self) -> bool {
        self.len() == 0
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
///    independent of [`len`](OrderedMap::len).
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
