//! A flat, dense-integer-keyed bijection backed by index `Vec`s — the radix
//! interner backend the research survey picked as the top candidate for dense,
//! (near-)monotonic ids (an SMT solver's `TermId ↔ VarId`). See [`FlatRadixBimap`].

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

/// A type usable as a dense `Vec` index in [`FlatRadixBimap`]: a cheap, total
/// `self -> usize` projection. Implemented for the unsigned integer primitives;
/// implement it for an id newtype (e.g. an SMT `TermId`/`VarId`) to use that
/// newtype as a `FlatRadixBimap` key or value.
///
/// Heap-free, so it is available in every tier even though `FlatRadixBimap`
/// itself needs `alloc`. (`usize`-conversion is `as`-based because the std
/// `From<u32> for usize` etc. do not exist on 16-bit targets; for a dense id
/// space the projection is lossless in practice.)
pub trait DenseIndex: Copy {
    /// The dense index this id maps to. Must be total and cheap.
    fn to_index(self) -> usize;
}

impl DenseIndex for u8 {
    #[inline]
    fn to_index(self) -> usize {
        self as usize
    }
}
impl DenseIndex for u16 {
    #[inline]
    fn to_index(self) -> usize {
        self as usize
    }
}
impl DenseIndex for u32 {
    #[inline]
    fn to_index(self) -> usize {
        self as usize
    }
}
impl DenseIndex for u64 {
    #[inline]
    fn to_index(self) -> usize {
        self as usize
    }
}
impl DenseIndex for usize {
    #[inline]
    fn to_index(self) -> usize {
        self
    }
}

ifstdoralloc!({
    use portable_collection_primitives::{Checkpoint, ScopedRollback, Bimap, Container};
    use crate::InsertError;

    /// A bijection `K ↔ V` for **dense integer** keys/values, backed by direct-
    /// indexed `Vec`s instead of trees.
    ///
    /// Where [`BTreeBimap`](crate::BTreeBimap) is generic over `K: Ord` and pays a
    /// comparison plus a cache miss per lookup, `FlatRadixBimap` requires
    /// `K, V: Copy + Into<usize>` and gives **O(1), one-cache-miss, zero-compare**
    /// lookups by using the id itself as the index. That is the ideal shape for an
    /// interner that mints dense, (near-)monotonic ids — the radix win an ART
    /// chases, with none of its node machinery.
    ///
    /// `fwd` is `Vec<Option<V>>` indexed by `K`; `rev` is `Vec<Option<K>>` indexed
    /// by `V`; `order` is the insertion log that drives rollback. Rolling back is a
    /// length-truncate of `order` plus clearing the popped ids' forward/reverse
    /// slots — satisfying the [`ScopedRollback`] contract by construction.
    ///
    /// ```
    /// use portable_bijectives::FlatRadixBimap;
    /// let mut m: FlatRadixBimap<u32, u32> = FlatRadixBimap::new();
    /// m.insert(10, 0).unwrap();          // term 10 ↔ var 0
    /// let scope = m.checkpoint();
    /// m.insert(11, 1).unwrap();          // inside the scope
    /// assert_eq!(m.get(&11), Some(&1));
    /// m.truncate(scope);                 // leave the scope: BOTH directions roll back
    /// assert_eq!(m.get(&11), None);
    /// assert_eq!(m.get_key(&1), None);
    /// assert_eq!(m.get(&10), Some(&0));  // the outer scope is intact
    /// ```
    #[derive(Clone)]
    pub struct FlatRadixBimap<K, V> {
        fwd: Vec<Option<V>>,
        rev: Vec<Option<K>>,
        /// Insertion order; also the rollback log (truncated from the back).
        order: Vec<(K, V)>,
    }

    impl<K, V> FlatRadixBimap<K, V> {
        /// Create an empty bimap.
        #[must_use]
        pub const fn new() -> Self {
            Self {
                fwd: Vec::new(),
                rev: Vec::new(),
                order: Vec::new(),
            }
        }

        /// Number of entries currently held.
        #[must_use]
        pub fn len(&self) -> usize {
            self.order.len()
        }

        /// Whether the bimap holds no entries.
        #[must_use]
        pub fn is_empty(&self) -> bool {
            self.order.is_empty()
        }

        /// An opaque checkpoint of the current size, for a later
        /// [`truncate`](Self::truncate); exactly the current [`len`](Self::len).
        #[must_use]
        pub fn checkpoint(&self) -> usize {
            self.order.len()
        }

        /// Like [`checkpoint`](Self::checkpoint) but returns the typed, opaque
        /// [`Checkpoint`] mark.
        #[must_use]
        pub fn checkpoint_mark(&self) -> Checkpoint {
            Checkpoint::from_len(self.order.len())
        }

        /// The entries in insertion order.
        #[must_use]
        pub fn entries(&self) -> &[(K, V)] {
            &self.order
        }

        /// Iterate `(key, value)` pairs in insertion order.
        pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
            self.order.iter().map(|(k, v)| (k, v))
        }

        /// Iterate keys in insertion order.
        pub fn keys(&self) -> impl Iterator<Item = &K> {
            self.order.iter().map(|(k, _)| k)
        }

        /// Iterate values in insertion order.
        pub fn values(&self) -> impl Iterator<Item = &V> {
            self.order.iter().map(|(_, v)| v)
        }
    }

    impl<K: DenseIndex, V: DenseIndex> FlatRadixBimap<K, V> {
        /// The value bound to `key`, if any (forward lookup). An id that was never
        /// inserted (out of range) returns `None` rather than panicking.
        #[must_use]
        pub fn get(&self, key: &K) -> Option<&V> {
            self.fwd.get((*key).to_index())?.as_ref()
        }

        /// The key bound to `value`, if any (reverse lookup).
        #[must_use]
        pub fn get_key(&self, value: &V) -> Option<&K> {
            self.rev.get((*value).to_index())?.as_ref()
        }

        /// Whether `key` is mapped.
        #[must_use]
        pub fn contains_key(&self, key: &K) -> bool {
            self.get(key).is_some()
        }

        /// Whether `value` is mapped.
        #[must_use]
        pub fn contains_value(&self, value: &V) -> bool {
            self.get_key(value).is_some()
        }

        /// Insert a fresh `key ↔ value` pair, preserving bijectivity. Returns
        /// [`InsertError`] (inserting nothing) if either side is already mapped; on
        /// success both index Vecs and the order log gain the entry together.
        pub fn insert(&mut self, key: K, value: V) -> Result<(), InsertError<K, V>> {
            if self.contains_key(&key) {
                return Err(InsertError::DuplicateKey(key, value));
            }
            if self.contains_value(&value) {
                return Err(InsertError::DuplicateValue(key, value));
            }
            let ki: usize = key.to_index();
            let vi: usize = value.to_index();
            if ki >= self.fwd.len() {
                self.fwd.resize(ki + 1, None);
            }
            if vi >= self.rev.len() {
                self.rev.resize(vi + 1, None);
            }
            self.fwd[ki] = Some(value);
            self.rev[vi] = Some(key);
            self.order.push((key, value));
            Ok(())
        }

        /// Roll back to `len` entries, clearing every later pair from forward,
        /// reverse, and the order log in reverse insertion order. A `len` greater
        /// than the current size is a no-op. The index Vecs keep their capacity;
        /// only the popped slots are set to `None`.
        pub fn truncate(&mut self, len: usize) {
            while self.order.len() > len {
                // `pop` cannot fail: the loop guard guarantees a remaining element.
                if let Some((k, v)) = self.order.pop() {
                    self.fwd[k.to_index()] = None;
                    self.rev[v.to_index()] = None;
                }
            }
        }

        /// Remove all entries.
        pub fn clear(&mut self) {
            self.fwd.clear();
            self.rev.clear();
            self.order.clear();
        }
    }

    impl<K, V> Default for FlatRadixBimap<K, V> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<K: DenseIndex, V: DenseIndex> Container for FlatRadixBimap<K, V> {
        fn clear(&mut self) {
            FlatRadixBimap::clear(self);
        }
        fn len(&self) -> usize {
            FlatRadixBimap::len(self)
        }
    }

    impl<K: DenseIndex, V: DenseIndex> ScopedRollback for FlatRadixBimap<K, V> {
        type Mark = Checkpoint;

        fn checkpoint(&self) -> Checkpoint {
            self.checkpoint_mark()
        }

        fn rollback_to(&mut self, mark: Checkpoint) {
            self.truncate(mark.as_len());
        }
    }

    impl<K: DenseIndex, V: DenseIndex> Bimap<K, V> for FlatRadixBimap<K, V> {
        type InsertError = InsertError<K, V>;

        fn get_by_key(&self, key: &K) -> Option<&V> {
            self.get(key)
        }

        fn get_by_value(&self, value: &V) -> Option<&K> {
            self.get_key(value)
        }

        fn insert(&mut self, key: K, value: V) -> Result<(), InsertError<K, V>> {
            FlatRadixBimap::insert(self, key, value)
        }

        fn iter<'a>(&'a self) -> impl Iterator<Item = (&'a K, &'a V)>
        where
            K: 'a,
            V: 'a,
        {
            FlatRadixBimap::iter(self)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn insert_and_both_lookups() {
            let mut m: FlatRadixBimap<u32, u32> = FlatRadixBimap::new();
            m.insert(10, 0).unwrap();
            m.insert(20, 1).unwrap();
            assert_eq!(m.get(&10), Some(&0));
            assert_eq!(m.get_key(&1), Some(&20));
            assert_eq!(m.get(&30), None); // out of range → None, no panic
            assert_eq!(m.get_key(&9), None);
            assert_eq!(m.len(), 2);
            assert!(!m.is_empty());
        }

        #[test]
        fn rejects_duplicate_key_or_value_without_mutating() {
            let mut m: FlatRadixBimap<u32, u32> = FlatRadixBimap::new();
            m.insert(10, 0).unwrap();
            assert!(matches!(m.insert(10, 9), Err(InsertError::DuplicateKey(10, 9))));
            assert!(matches!(m.insert(99, 0), Err(InsertError::DuplicateValue(99, 0))));
            assert_eq!(m.len(), 1);
            assert_eq!(m.get(&99), None);
        }

        #[test]
        fn truncate_rolls_back_both_directions_and_order() {
            let mut m: FlatRadixBimap<u32, u32> = FlatRadixBimap::new();
            m.insert(10, 0).unwrap();
            let scope = m.checkpoint();
            m.insert(11, 1).unwrap();
            m.insert(12, 2).unwrap();
            assert_eq!(m.len(), 3);
            m.truncate(scope);
            assert_eq!(m.len(), 1);
            assert_eq!(m.get(&11), None);
            assert_eq!(m.get(&12), None);
            assert_eq!(m.get_key(&1), None);
            assert_eq!(m.get_key(&2), None);
            assert_eq!(m.get(&10), Some(&0));
            assert_eq!(m.entries(), &[(10, 0)]);
        }

        #[test]
        fn reintern_after_truncate_gets_a_fresh_value() {
            let mut m: FlatRadixBimap<u32, u32> = FlatRadixBimap::new();
            let scope = m.checkpoint();
            m.insert(7, 36).unwrap();
            m.truncate(scope);
            assert_eq!(m.get(&7), None);
            m.insert(7, 5).unwrap();
            assert_eq!(m.get(&7), Some(&5));
            assert_eq!(m.get_key(&5), Some(&7));
            assert_eq!(m.get_key(&36), None);
        }

        #[test]
        fn iteration_is_in_insertion_order() {
            let mut m: FlatRadixBimap<u32, u32> = FlatRadixBimap::new();
            m.insert(30, 0).unwrap();
            m.insert(10, 1).unwrap();
            m.insert(20, 2).unwrap();
            let ks: Vec<u32> = m.keys().copied().collect();
            let vs: Vec<u32> = m.values().copied().collect();
            assert_eq!(ks, [30, 10, 20]); // insertion order, NOT sorted
            assert_eq!(vs, [0, 1, 2]);
        }

        #[test]
        fn bimap_trait_path() {
            use portable_collection_primitives::{Bimap, Container, ScopedRollback, Checkpoint};
            let mut m: FlatRadixBimap<u32, u32> = FlatRadixBimap::new();
            Bimap::insert(&mut m, 1, 10).unwrap();
            Bimap::insert(&mut m, 2, 20).unwrap();
            assert_eq!(Bimap::get_by_key(&m, &1), Some(&10));
            assert_eq!(Bimap::get_by_value(&m, &20), Some(&2));
            assert_eq!(Container::len(&m), 2);
            assert!(!Container::is_empty(&m));
            let pairs: Vec<(u32, u32)> = Bimap::iter(&m).map(|(&k, &v)| (k, v)).collect();
            assert_eq!(pairs, [(1, 10), (2, 20)]);
            let mark: Checkpoint = ScopedRollback::checkpoint(&m);
            Bimap::insert(&mut m, 3, 30).unwrap();
            ScopedRollback::rollback_to(&mut m, mark);
            assert_eq!(Container::len(&m), 2);
            assert_eq!(Bimap::get_by_key(&m, &3), None);
            Container::clear(&mut m);
            assert!(Container::is_empty(&m));
        }
    }
});
