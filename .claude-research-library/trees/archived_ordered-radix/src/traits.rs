//! The map interface: `insert / lookup / range / snapshot`.

use alloc::vec::Vec;

/// An ordered map keyed by byte slices.
pub trait OrderedMap<V> {
    /// Insert / overwrite `key -> value`; returns the previous value if any.
    fn insert(&mut self, key: &[u8], value: V) -> Option<V>;

    /// Point lookup.
    fn get(&self, key: &[u8]) -> Option<&V>;

    /// Ordered scan over the inclusive byte-range `[lo, hi]`, ascending by key.
    fn range(&self, lo: &[u8], hi: &[u8]) -> Vec<(Vec<u8>, V)>
    where
        V: Clone;

    /// Number of stored keys.
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A map supporting O(1) snapshots (an immutable, isolated point-in-time view).
pub trait SnapshotMap<V>: OrderedMap<V> {
    /// The snapshot type (itself an [`OrderedMap`] for reads).
    type Snapshot: OrderedMap<V>;

    /// Take a snapshot. For a copy-on-write structure this is O(1) and shares
    /// structure with the live map; later writes to the live map do not affect
    /// the snapshot.
    fn snapshot(&self) -> Self::Snapshot;
}
