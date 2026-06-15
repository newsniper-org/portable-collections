//! The bijection contract shared by the workspace's bimaps (`BTreeBimap` today;
//! the planned dense `FlatRadixBimap` next).

use super::ScopedRollback;

/// A bijection `K ↔ V` — one key ↔ one value, **both** directions are lookups —
/// that also remembers insertion order and rolls back atomically to a checkpoint
/// (via the [`ScopedRollback`] supertrait, which carries `checkpoint`,
/// `rollback_to`, and `clear`).
///
/// Unlike the [`Map`](super::Map) / [`MapShim`](super::MapShim) facade, `Bimap`
/// takes owned `&K` / `&V` directly with **no `Borrow<Q>` shim**: a bijection is
/// queried by its actual key/value (e.g. an interner's `TermId` / `VarId`), so
/// the per-backend `Q`-bound machinery the `Map` facade needs — `MapShim` exists
/// only because `BTreeMap::get<Q>` wants `Q: Ord` while `HashMap::get<Q>` wants
/// `Q: Hash + Eq`, and a single trait cannot express that — is simply absent here.
///
/// The trait is **heap-free** (it touches neither `alloc` nor `std`), so it is
/// present in every feature tier; only the concrete bimaps that implement it need
/// `alloc`.
pub trait Bimap<K, V>: ScopedRollback {
    /// The error an [`insert`](Self::insert) returns when the pair would break
    /// bijectivity. Each backend supplies its own (e.g. a `DuplicateKey` /
    /// `DuplicateValue` enum carrying the rejected pair back to the caller).
    type InsertError;

    /// Forward lookup: the value bound to `key`, if any.
    #[must_use]
    fn get_by_key(&self, key: &K) -> Option<&V>;

    /// Reverse lookup: the key bound to `value`, if any.
    #[must_use]
    fn get_by_value(&self, value: &V) -> Option<&K>;

    /// Insert a fresh `key ↔ value` pair, preserving bijectivity. On rejection
    /// (either side already mapped) returns [`Self::InsertError`] and inserts
    /// nothing; on success both directions and the order log gain the pair
    /// together.
    fn insert(&mut self, key: K, value: V) -> Result<(), Self::InsertError>;

    /// Iterate `(key, value)` pairs in insertion order (the order a later
    /// [`rollback_to`](ScopedRollback::rollback_to) peels back from the end).
    fn iter<'a>(&'a self) -> impl Iterator<Item = (&'a K, &'a V)>
    where
        K: 'a,
        V: 'a;
}
