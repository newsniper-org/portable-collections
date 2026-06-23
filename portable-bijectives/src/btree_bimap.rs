//! Portable, `no_std` bijective data structures with scope-checkpointed rollback.
//!
//! The crate is `#![no_std]` by default and depends only on `alloc`. Enable the
//! `std` feature to additionally get `std::error::Error` impls for the error
//! types.
//!
//! # [`BTreeBimap`]
//!
//! A **bijection** `K ↔ V` (one key ↔ one value, both ways) that also remembers
//! its **insertion order** and can be **rolled back to a checkpoint** in a single
//! atomic operation. It exists to make one specific bug class *unrepresentable*:
//! a manually-maintained pair of maps `K → V` and `… → K` where a scope `pop`
//! rolls back one map but forgets the other, leaving a *stale* mapping that a
//! later lookup returns. Here both directions — and the order log used for
//! rollback — live in one type and are only ever mutated together, so they can
//! never desync.
//!
//! ```
//! use portable_bijectives::BTreeBimap;
//! let mut m: BTreeBimap<u32, u32> = BTreeBimap::new();
//! m.insert(10, 0).unwrap();          // term 10 ↔ var 0
//! let scope = m.checkpoint();        // remember where the scope began
//! m.insert(11, 1).unwrap();          // term 11 ↔ var 1 (inside the scope)
//! assert_eq!(m.get(&11), Some(&1));
//! m.truncate(scope);                 // leave the scope: BOTH directions roll back
//! assert_eq!(m.get(&11), None);      // no stale forward mapping …
//! assert_eq!(m.get_key(&1), None);   // … and no stale reverse mapping
//! assert_eq!(m.get(&10), Some(&0));  // the outer scope is intact
//! ```

use portable_collection_primitives::{ifstd, ifstdoralloc};

ifstd!({
    use std::collections::BTreeMap;
    use std::vec::Vec;
    use std::fmt;
    use portable_collection_primitives::implgroup_for;

    #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
    use std::alloc::{Allocator, Global};

    #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
    use std::clone::Clone;

    use std::iter::{IntoIterator, Iterator};

} else {
    use portable_collection_primitives::ifalloc;
    ifalloc!({
        extern crate alloc;
        use alloc::collections::BTreeMap;
        use alloc::vec::Vec;
        use core::fmt;

        #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
        use core::alloc::Allocator;

        #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
        use alloc::alloc::Global;

        #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
        use core::clone::Clone;
    });
});



ifstdoralloc!({
    use portable_collection_primitives::{Checkpoint, ScopedRollback, Bimap, Container, Clearable};

    /// A scope-checkpointed bijection `K ↔ V` with an insertion-order log.
    ///
    /// Every entry is a 1-to-1 pair: a key maps to exactly one value and a value to
    /// exactly one key. [`insert`](Self::insert) refuses to break that invariant.
    /// [`checkpoint`](Self::checkpoint) returns an opaque mark (the current length)
    /// and [`truncate`](Self::truncate) rolls every later entry out of **both**
    /// lookup directions at once — the operation that makes a forward/reverse
    /// desync impossible.
    ///
    /// Backed by `alloc::collections::BTreeMap`, so it is `no_std`-friendly,
    /// dependency-free, and iterates in deterministic (insertion) order via
    /// [`iter`](Self::iter) / [`entries`](Self::entries).
    #[derive(Clone)]
    pub struct BTreeBimap<K, V, #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))] A: Allocator + Clone = Global> {
        #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
        fwd: BTreeMap<K, V, A>,
        #[cfg(not(all(feature = "unstable", not(toolchain_channel = "stable"))))]
        fwd: BTreeMap<K, V>,
        #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
        rev: BTreeMap<V, K, A>,
        #[cfg(not(all(feature = "unstable", not(toolchain_channel = "stable"))))]
        rev: BTreeMap<V, K>,
        /// Insertion order; also the rollback log (truncated from the back).
        #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
        order: Vec<(K, V), A>,
        #[cfg(not(all(feature = "unstable", not(toolchain_channel = "stable"))))]
        order: Vec<(K, V)>,
    }
    
    #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
    impl<K, V, A: Allocator + Clone> BTreeBimap<K, V, A> {
        /// Create an empty bimap.
        pub const fn new_in(alloc0: A, alloc1: A, alloc2: A) -> Self {
            Self {
                fwd: BTreeMap::new_in(alloc0),
                rev: BTreeMap::new_in(alloc1),
                order: Vec::<(K, V), A>::new_in(alloc2),
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
        /// [`truncate`](Self::truncate). It is exactly the current [`len`](Self::len);
        /// callers that nest scopes save one of these per scope.
        ///
        /// Note: with [`ScopedRollback`] in scope, `m.checkpoint()` still
        /// resolves to *this* inherent method (returning `usize`) — inherent
        /// methods shadow trait methods in call syntax. For the typed
        /// [`Checkpoint`] mark use [`checkpoint_mark`](Self::checkpoint_mark) or
        /// the fully-qualified `ScopedRollback::checkpoint(&m)`.
        #[must_use]
        pub fn checkpoint(&self) -> usize {
            self.order.len()
        }

        /// Like [`checkpoint`](Self::checkpoint) but returns a typed, opaque
        /// [`Checkpoint`] mark — recommended for callers who store a mark
        /// alongside other counts and want the type to stop a mix-up (e.g. an
        /// SMT solver keeping it next to `num_vars`). The bare-`usize`
        /// [`checkpoint`](Self::checkpoint) stays for index-flavored callers.
        #[must_use]
        pub fn checkpoint_mark(&self) -> Checkpoint {
            Checkpoint::from_len(self.order.len())
        }

        /// The entries in insertion order (the same order [`iter`](Self::iter)
        /// yields). The slice index is **not** a stable id — it shifts under
        /// [`truncate`](Self::truncate); use the key/value for identity.
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

        /// Remove all entries.
        pub fn clear(&mut self) {
            self.fwd.clear();
            self.rev.clear();
            self.order.clear();
        }
    }
    
    #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
    impl<K, V, A: Allocator + Clone> IntoIterator for BTreeBimap<K, V, A> {
        type Item = (K, V);
        type IntoIter = <Vec<Self::Item, A> as IntoIterator>::IntoIter;

        fn into_iter(self) -> Self::IntoIter {
            self.order.into_iter()
        }
    }

    #[cfg(not(all(feature = "unstable", not(toolchain_channel = "stable"))))]
    impl<K, V> IntoIterator for BTreeBimap<K, V> {
        type Item = (K, V);
        type IntoIter = <Vec<Self::Item> as IntoIterator>::IntoIter;

        fn into_iter(self) -> Self::IntoIter {
            self.order.into_iter()
        }
    }

    #[cfg(not(all(feature = "unstable", not(toolchain_channel = "stable"))))]
    impl<K, V> BTreeBimap<K, V> {
        /// Create an empty bimap.
        #[must_use]
        pub const fn new() -> Self {
            Self {
                fwd: BTreeMap::new(),
                rev: BTreeMap::new(),
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
        /// [`truncate`](Self::truncate). It is exactly the current [`len`](Self::len);
        /// callers that nest scopes save one of these per scope.
        ///
        /// Note: with [`ScopedRollback`] in scope, `m.checkpoint()` still
        /// resolves to *this* inherent method (returning `usize`) — inherent
        /// methods shadow trait methods in call syntax. For the typed
        /// [`Checkpoint`] mark use [`checkpoint_mark`](Self::checkpoint_mark) or
        /// the fully-qualified `ScopedRollback::checkpoint(&m)`.
        #[must_use]
        pub fn checkpoint(&self) -> usize {
            self.order.len()
        }

        /// Like [`checkpoint`](Self::checkpoint) but returns a typed, opaque
        /// [`Checkpoint`] mark — recommended for callers who store a mark
        /// alongside other counts and want the type to stop a mix-up (e.g. an
        /// SMT solver keeping it next to `num_vars`). The bare-`usize`
        /// [`checkpoint`](Self::checkpoint) stays for index-flavored callers.
        #[must_use]
        pub fn checkpoint_mark(&self) -> Checkpoint {
            Checkpoint::from_len(self.order.len())
        }

        /// The entries in insertion order (the same order [`iter`](Self::iter)
        /// yields). The slice index is **not** a stable id — it shifts under
        /// [`truncate`](Self::truncate); use the key/value for identity.
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

        /// Remove all entries.
        pub fn clear(&mut self) {
            self.fwd.clear();
            self.rev.clear();
            self.order.clear();
        }
    }

    #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
    impl<K: Ord + Clone, V: Ord + Clone, A: Allocator + Clone> BTreeBimap<K, V, A> {
        /// The value bound to `key`, if any (forward lookup).
        #[must_use]
        pub fn get(&self, key: &K) -> Option<&V> {
            self.fwd.get(key)
        }

        /// The key bound to `value`, if any (reverse lookup).
        #[must_use]
        pub fn get_key(&self, value: &V) -> Option<&K> {
            self.rev.get(value)
        }

        /// Whether `key` is mapped.
        #[must_use]
        pub fn contains_key(&self, key: &K) -> bool {
            self.fwd.contains_key(key)
        }

        /// Whether `value` is mapped.
        #[must_use]
        pub fn contains_value(&self, value: &V) -> bool {
            self.rev.contains_key(value)
        }

        /// Insert a fresh `key ↔ value` pair, preserving bijectivity.
        ///
        /// Returns [`InsertError::DuplicateKey`] if `key` is already mapped, or
        /// [`InsertError::DuplicateValue`] if `value` is already mapped (in either
        /// case nothing is inserted). On success both directions and the order log
        /// gain the entry together.
        pub fn insert(&mut self, key: K, value: V) -> Result<(), InsertError<K, V>> {
            if self.fwd.contains_key(&key) {
                return Err(InsertError::DuplicateKey(key, value));
            }
            if self.rev.contains_key(&value) {
                return Err(InsertError::DuplicateValue(key, value));
            }
            self.fwd.insert(key.clone(), value.clone());
            self.rev.insert(value.clone(), key.clone());
            self.order.push((key, value));
            Ok(())
        }

        /// Roll back to `len` entries, dropping every later entry from **both** the
        /// forward and reverse maps and the order log, atomically. Removal is in
        /// reverse insertion order. A `len` greater than the current size is a no-op.
        pub fn truncate(&mut self, len: usize) {
            while self.order.len() > len {
                // `pop` cannot fail: the loop guard guarantees a remaining element.
                if let Some((k, v)) = self.order.pop() {
                    self.fwd.remove(&k);
                    self.rev.remove(&v);
                }
            }
        }
    }

    #[cfg(not(all(feature = "unstable", not(toolchain_channel = "stable"))))]
    impl<K: Ord + Clone, V: Ord + Clone> BTreeBimap<K, V> {
        /// The value bound to `key`, if any (forward lookup).
        #[must_use]
        pub fn get(&self, key: &K) -> Option<&V> {
            self.fwd.get(key)
        }

        /// The key bound to `value`, if any (reverse lookup).
        #[must_use]
        pub fn get_key(&self, value: &V) -> Option<&K> {
            self.rev.get(value)
        }

        /// Whether `key` is mapped.
        #[must_use]
        pub fn contains_key(&self, key: &K) -> bool {
            self.fwd.contains_key(key)
        }

        /// Whether `value` is mapped.
        #[must_use]
        pub fn contains_value(&self, value: &V) -> bool {
            self.rev.contains_key(value)
        }

        /// Insert a fresh `key ↔ value` pair, preserving bijectivity.
        ///
        /// Returns [`InsertError::DuplicateKey`] if `key` is already mapped, or
        /// [`InsertError::DuplicateValue`] if `value` is already mapped (in either
        /// case nothing is inserted). On success both directions and the order log
        /// gain the entry together.
        pub fn insert(&mut self, key: K, value: V) -> Result<(), InsertError<K, V>> {
            if self.fwd.contains_key(&key) {
                return Err(InsertError::DuplicateKey(key, value));
            }
            if self.rev.contains_key(&value) {
                return Err(InsertError::DuplicateValue(key, value));
            }
            self.fwd.insert(key.clone(), value.clone());
            self.rev.insert(value.clone(), key.clone());
            self.order.push((key, value));
            Ok(())
        }

        /// Roll back to `len` entries, dropping every later entry from **both** the
        /// forward and reverse maps and the order log, atomically. Removal is in
        /// reverse insertion order. A `len` greater than the current size is a no-op.
        pub fn truncate(&mut self, len: usize) {
            while self.order.len() > len {
                // `pop` cannot fail: the loop guard guarantees a remaining element.
                if let Some((k, v)) = self.order.pop() {
                    self.fwd.remove(&k);
                    self.rev.remove(&v);
                }
            }
        }
    }

    /// The shared scope-rollback contract. `checkpoint`/`rollback_to` are the
    /// typed-mark twins of the inherent `checkpoint`/`truncate`: every backing
    /// store (`fwd`, `rev`, `order`) rolls back together, satisfying the
    /// [`ScopedRollback`] five-law contract (round-trip, atomic-across-stores,
    /// LIFO, overshoot-no-op, idempotent). The inherent `usize` methods are kept
    /// for index-flavored callers; this impl adds the desync-proof typed path.
    #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
    impl<K: Ord + Clone, V: Ord + Clone, A: Allocator + Clone> ScopedRollback for BTreeBimap<K, V, A> {
        type Mark = Checkpoint;

        fn checkpoint(&self) -> Checkpoint {
            self.checkpoint_mark()
        }

        fn rollback_to(&mut self, mark: Checkpoint) {
            self.truncate(mark.as_len());
        }
    }

    #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
    impl<K, V, A: Allocator + Clone> Container for BTreeBimap<K, V, A> {
        fn clear(&mut self) {
            // fwd + rev + order together.
            BTreeBimap::clear(self);
        }
        fn len(&self) -> usize {
            BTreeBimap::len(self)
        }
    }

    #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
    impl<K: Ord + Clone, V: Ord + Clone, A: Allocator + Clone> Bimap<K, V> for BTreeBimap<K, V, A> {
        type InsertError = InsertError<K, V>;

        fn get_by_key(&self, key: &K) -> Option<&V> {
            self.get(key)
        }

        fn get_by_value(&self, value: &V) -> Option<&K> {
            self.get_key(value)
        }

        fn insert(&mut self, key: K, value: V) -> Result<(), InsertError<K, V>> {
            BTreeBimap::insert(self, key, value)
        }

        fn iter<'a>(&'a self) -> impl Iterator<Item = (&'a K, &'a V)>
        where
            K: 'a,
            V: 'a,
        {
            BTreeBimap::iter(self)
        }
    }
    #[cfg(not(all(feature = "unstable", not(toolchain_channel = "stable"))))]
    impl<K: Ord + Clone, V: Ord + Clone> ScopedRollback for BTreeBimap<K, V> {
        type Mark = Checkpoint;

        fn checkpoint(&self) -> Checkpoint {
            self.checkpoint_mark()
        }

        fn rollback_to(&mut self, mark: Checkpoint) {
            self.truncate(mark.as_len());
        }
    }

    #[cfg(not(all(feature = "unstable", not(toolchain_channel = "stable"))))]
    impl<K, V> Container for BTreeBimap<K, V> {
        fn len(&self) -> usize {
            BTreeBimap::len(self)
        }
    }

    #[cfg(not(all(feature = "unstable", not(toolchain_channel = "stable"))))]
    impl<K, V> Clearable for BTreeBimap<K, V> {
        fn clear(&mut self) {
            // fwd + rev + order together.
            BTreeBimap::clear(self);
        }
    }

    #[cfg(not(all(feature = "unstable", not(toolchain_channel = "stable"))))]
    impl<K: Ord + Clone, V: Ord + Clone> Bimap<K, V> for BTreeBimap<K, V> {
        type InsertError = InsertError<K, V>;

        fn get_by_key(&self, key: &K) -> Option<&V> {
            self.get(key)
        }

        fn get_by_value(&self, value: &V) -> Option<&K> {
            self.get_key(value)
        }

        fn insert(&mut self, key: K, value: V) -> Result<(), InsertError<K, V>> {
            BTreeBimap::insert(self, key, value)
        }

        fn iter<'a>(&'a self) -> impl Iterator<Item = (&'a K, &'a V)>
        where
            K: 'a,
            V: 'a,
        {
            BTreeBimap::iter(self)
        }
    }


    #[cfg(not(all(feature = "unstable", not(toolchain_channel = "stable"))))]
    impl<K, V> Default for BTreeBimap<K, V> {
        fn default() -> Self {
            Self::new()
        }
    }

    #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
    impl<K: fmt::Debug, V: fmt::Debug, A: Allocator + Clone> fmt::Debug for BTreeBimap<K, V, A> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_map().entries(self.order.iter().map(|(k, v)| (k, v))).finish()
        }
    }

    #[cfg(not(all(feature = "unstable", not(toolchain_channel = "stable"))))]
    impl<K: fmt::Debug, V: fmt::Debug> fmt::Debug for BTreeBimap<K, V> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_map().entries(self.order.iter().map(|(k, v)| (k, v))).finish()
        }
    }

    /// Why an [`insert`](BTreeBimap::insert) was refused. Carries back the rejected
    /// `(key, value)` so the caller can recover or report them.
    #[derive(Clone, PartialEq, Eq)]
    pub enum InsertError<K, V> {
        /// `key` was already mapped (to some value). Bijectivity forbids re-mapping
        /// it without a [`truncate`](BTreeBimap::truncate) or
        /// [`clear`](BTreeBimap::clear) first.
        DuplicateKey(K, V),
        /// `value` was already mapped (from some key).
        DuplicateValue(K, V),
    }

    impl<K: fmt::Debug, V: fmt::Debug> fmt::Debug for InsertError<K, V> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::DuplicateKey(k, v) => {
                    write!(f, "DuplicateKey({k:?} -> {v:?}: key already mapped)")
                }
                Self::DuplicateValue(k, v) => {
                    write!(f, "DuplicateValue({k:?} -> {v:?}: value already mapped)")
                }
            }
        }
    }

    impl<K: fmt::Debug, V: fmt::Debug> fmt::Display for InsertError<K, V> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::DuplicateKey(..) => f.write_str("key already mapped (bijection violation)"),
                Self::DuplicateValue(..) => f.write_str("value already mapped (bijection violation)"),
            }
        }
    }

    ifstd!({
        implgroup_for! {
            { InsertError<K, V> }
            {
                impl<K: fmt::Debug, V: fmt::Debug> ::std::error::Error {}
            }
        }
    });

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn insert_and_both_lookups() {
            let mut m: BTreeBimap<u32, u32> = BTreeBimap::new();
            m.insert(10, 0).unwrap();
            m.insert(20, 1).unwrap();
            assert_eq!(m.get(&10), Some(&0));
            assert_eq!(m.get(&20), Some(&1));
            assert_eq!(m.get_key(&0), Some(&10));
            assert_eq!(m.get_key(&1), Some(&20));
            assert_eq!(m.get(&30), None);
            assert_eq!(m.get_key(&2), None);
            assert_eq!(m.len(), 2);
            assert!(!m.is_empty());
        }

        #[test]
        fn rejects_duplicate_key_or_value_without_mutating() {
            let mut m: BTreeBimap<u32, u32> = BTreeBimap::new();
            m.insert(10, 0).unwrap();
            assert!(matches!(m.insert(10, 9), Err(InsertError::DuplicateKey(10, 9))));
            assert!(matches!(m.insert(99, 0), Err(InsertError::DuplicateValue(99, 0))));
            // The failed inserts left nothing behind.
            assert_eq!(m.len(), 1);
            assert_eq!(m.get(&10), Some(&0));
            assert_eq!(m.get(&99), None);
            assert_eq!(m.get_key(&9), None);
        }

        #[test]
        fn truncate_rolls_back_both_directions_and_order() {
            // The regression for the bug this type exists to prevent: rolling back
            // must clear the FORWARD map, the REVERSE map, AND the order log.
            let mut m: BTreeBimap<u32, u32> = BTreeBimap::new();
            m.insert(10, 0).unwrap();
            let scope = m.checkpoint();
            m.insert(11, 1).unwrap();
            m.insert(12, 2).unwrap();
            assert_eq!(m.len(), 3);

            m.truncate(scope);
            assert_eq!(m.len(), 1);
            // Forward gone.
            assert_eq!(m.get(&11), None);
            assert_eq!(m.get(&12), None);
            // Reverse gone (the half a manual two-map pop famously forgets).
            assert_eq!(m.get_key(&1), None);
            assert_eq!(m.get_key(&2), None);
            // Outer scope intact, both ways.
            assert_eq!(m.get(&10), Some(&0));
            assert_eq!(m.get_key(&0), Some(&10));
            // Order log reflects the rollback.
            assert_eq!(m.entries(), &[(10, 0)]);
        }

        #[test]
        fn reintern_after_truncate_gets_a_fresh_value() {
            // The exact arith-solver scenario: a term interned in a popped scope
            // can be re-interned afterwards to a NEW (freshly allocated) value, with
            // no stale mapping to the old one.
            let mut m: BTreeBimap<u32, u32> = BTreeBimap::new();
            let scope = m.checkpoint();
            m.insert(/*term*/ 7, /*var*/ 36).unwrap();
            assert_eq!(m.get(&7), Some(&36));
            m.truncate(scope); // pop the scope
            assert_eq!(m.get(&7), None); // the stale var-36 mapping is gone
            m.insert(/*term*/ 7, /*var*/ 5).unwrap(); // re-intern to a fresh var
            assert_eq!(m.get(&7), Some(&5));
            assert_eq!(m.get_key(&5), Some(&7));
            assert_eq!(m.get_key(&36), None); // and 36 was never resurrected
        }

        #[test]
        fn nested_scopes() {
            let mut m: BTreeBimap<u32, u32> = BTreeBimap::new();
            m.insert(1, 10).unwrap();
            let s1 = m.checkpoint();
            m.insert(2, 20).unwrap();
            let s2 = m.checkpoint();
            m.insert(3, 30).unwrap();
            assert_eq!(m.len(), 3);
            m.truncate(s2);
            assert_eq!(m.len(), 2);
            assert_eq!(m.get(&3), None);
            assert_eq!(m.get(&2), Some(&20));
            m.truncate(s1);
            assert_eq!(m.len(), 1);
            assert_eq!(m.get(&2), None);
            assert_eq!(m.get(&1), Some(&10));
        }

        #[test]
        fn truncate_past_end_is_noop_and_to_zero_clears() {
            let mut m: BTreeBimap<u32, u32> = BTreeBimap::new();
            m.insert(1, 10).unwrap();
            m.insert(2, 20).unwrap();
            m.truncate(99); // larger than len → no-op
            assert_eq!(m.len(), 2);
            m.truncate(0);
            assert!(m.is_empty());
            assert_eq!(m.get(&1), None);
            assert_eq!(m.get_key(&10), None);
            assert_eq!(m.entries(), &[]);
        }

        #[test]
        fn clear_empties_all_three() {
            let mut m: BTreeBimap<u32, u32> = BTreeBimap::new();
            m.insert(1, 10).unwrap();
            m.insert(2, 20).unwrap();
            m.clear();
            assert!(m.is_empty());
            assert_eq!(m.get(&1), None);
            assert_eq!(m.get_key(&20), None);
        }

        #[test]
        fn iteration_is_in_insertion_order() {
            let mut m: BTreeBimap<u32, u32> = BTreeBimap::new();
            m.insert(30, 0).unwrap();
            m.insert(10, 1).unwrap();
            m.insert(20, 2).unwrap();
            let ks: Vec<u32> = m.keys().copied().collect();
            let vs: Vec<u32> = m.values().copied().collect();
            // Compare against array literals (not `alloc::vec!`) so the test
            // builds under the `std` import branch too, where `alloc` isn't
            // `extern crate`-d. `Vec<u32>: PartialEq<[u32; N]>` covers this.
            assert_eq!(ks, [30, 10, 20]); // insertion order, NOT sorted
            assert_eq!(vs, [0, 1, 2]);
        }

        #[test]
        fn works_with_non_integer_keys() {
            // Generic over any Ord+Clone key/value, e.g. string keys.
            let mut m: BTreeBimap<&'static str, u8> = BTreeBimap::new();
            m.insert("x", 0).unwrap();
            m.insert("y", 1).unwrap();
            assert_eq!(m.get(&"x"), Some(&0));
            assert_eq!(m.get_key(&1), Some(&"y"));
        }

        #[test]
        fn scoped_rollback_trait_path_satisfies_contract() {
            // Exercises the `ScopedRollback` trait API directly — the tests
            // above only cover the inherent `usize`/`truncate` path. Verifies
            // the five-law contract through the typed-mark methods.
            use portable_collection_primitives::{Checkpoint, ScopedRollback};
            let mut m: BTreeBimap<u32, u32> = BTreeBimap::new();
            m.insert(10, 0).unwrap();
            let mark: Checkpoint = ScopedRollback::checkpoint(&m);
            m.insert(11, 1).unwrap();
            m.insert(12, 2).unwrap();
            // Laws 1 + 2: rollback_to drops everything after the mark, in BOTH
            // directions, atomically.
            ScopedRollback::rollback_to(&mut m, mark);
            assert_eq!(m.len(), 1);
            assert_eq!(m.get(&11), None);
            assert_eq!(m.get(&12), None);
            assert_eq!(m.get_key(&1), None);
            assert_eq!(m.get_key(&2), None);
            assert_eq!(m.get(&10), Some(&0));
            assert_eq!(m.get_key(&0), Some(&10));
            // Law 5: idempotent.
            ScopedRollback::rollback_to(&mut m, mark);
            assert_eq!(m.len(), 1);
            // Law 4: a mark at/beyond the current size is a no-op.
            ScopedRollback::rollback_to(&mut m, Checkpoint::from_len(999));
            assert_eq!(m.len(), 1);
            // ORIGIN rolls all the way back (empties every store).
            ScopedRollback::rollback_to(&mut m, Checkpoint::ORIGIN);
            assert!(m.is_empty());
            assert_eq!(m.get(&10), None);
            assert_eq!(m.get_key(&0), None);
        }

        #[test]
        fn checkpoint_mark_agrees_with_inherent_and_trait_clear_empties() {
            use portable_collection_primitives::{Checkpoint, Clearable};
            let mut m: BTreeBimap<u32, u32> = BTreeBimap::new();
            m.insert(1, 10).unwrap();
            m.insert(2, 20).unwrap();
            // The typed mark and the bare-usize checkpoint agree on the length.
            assert_eq!(m.checkpoint_mark(), Checkpoint::from_len(m.checkpoint()));
            // `clear` now lives on the `Container` supertrait; it still delegates
            // to the inherent clear (all three stores).
            Clearable::clear(&mut m);
            assert!(m.is_empty());
            assert_eq!(m.get(&1), None);
            assert_eq!(m.get_key(&20), None);
        }

        #[test]
        fn bimap_trait_path() {
            // Exercises the `Bimap` facade end-to-end: lookups/insert/iter on
            // `Bimap`, len/is_empty/clear from the `Container` supertrait, and
            // checkpoint/rollback from the `ScopedRollback` supertrait.
            use portable_collection_primitives::{Bimap, Clearable, ScopedRollback, Checkpoint};
            let mut m: BTreeBimap<u32, u32> = BTreeBimap::new();
            Bimap::insert(&mut m, 1, 10).unwrap();
            Bimap::insert(&mut m, 2, 20).unwrap();
            // Both directions, owned (no Borrow shim).
            assert_eq!(Bimap::get_by_key(&m, &1), Some(&10));
            assert_eq!(Bimap::get_by_value(&m, &20), Some(&2));
            // Container supertrait.
            assert_eq!(Container::len(&m), 2);
            assert!(!Container::is_empty(&m));
            // iter() in insertion order.
            let pairs: Vec<(u32, u32)> = Bimap::iter(&m).map(|(&k, &v)| (k, v)).collect();
            assert_eq!(pairs, [(1, 10), (2, 20)]);
            // checkpoint/rollback via the ScopedRollback supertrait.
            let mark: Checkpoint = ScopedRollback::checkpoint(&m);
            Bimap::insert(&mut m, 3, 30).unwrap();
            ScopedRollback::rollback_to(&mut m, mark);
            assert_eq!(Container::len(&m), 2);
            assert_eq!(Bimap::get_by_key(&m, &3), None);
            // Container::clear empties every store.
            Clearable::clear(&mut m);
            assert!(Container::is_empty(&m));
        }
    }
});