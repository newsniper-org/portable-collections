mod container;
pub use container::{Container, Clearable};
mod checkpointing;
pub use checkpointing::{Checkpoint, ScopedRollback};
mod bijection;
pub use bijection::Bimap;
mod scoped_log;
pub use scoped_log::{ScopedStack, ScopedQueue};

mod queue;
pub use queue::{Push, TryPush, Pop, Pull};

// The ordered byte-keyed map vocabulary (`range` + `snapshot`) needs `alloc`
// (its `range` iterator yields owned `Vec<u8>` keys), so it is gated to the
// alloc/std tiers — like the `Map`/`Set` re-exports.
ifstdoralloc!({
    mod ordered;
    pub use ordered::{OrderedMap, SnapshotMap};
});

// `core::borrow::Borrow` is also reachable as `std::borrow::Borrow`; living
// inside `ifstdoralloc!` keeps it out of the bare-no_std build (where the
// Map/Set traits that use it are absent), avoiding an unused-import warning.
use core::borrow::Borrow;

pub trait Map<K, V> : Container {
    fn get<Q: ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Self: MapShim<K, Q, V> {
        <Self as MapReadShim<Q, V>>::get(self, key)
    }
    fn get_mut<Q: ?Sized>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Self: MapShim<K, Q, V> {
        <Self as MapShim<K, Q, V>>::get_mut(self, key)
    }
    fn remove<Q: ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Self: MapShim<K, Q, V> {
        <Self as MapShim<K, Q, V>>::remove(self, key)
    }

    fn insert(&mut self, key: K, value: V) -> Option<V>
    where
        Self: MapShim<K, K, V> {
        <Self as MapInsertShim<K, K, V>>::insert(self, key, value)
    }

    fn contains_key<Q: ?Sized>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Self: MapShim<K, Q, V> {
        <Self as MapReadShim<Q, V>>::contains_key(self, key)
    }
}

pub trait MapReadShim<Q: ?Sized, V> : Container {
    fn get(&self, key: &Q) -> Option<&V>;
    fn contains_key(&self, key: &Q) -> bool;
}

pub trait MapRefKeyInsertShim<Q: ?Sized, V> : MapReadShim<Q, V> + Clearable {
    fn insert(&mut self, key: &Q, value: V) -> Option<V>;
}

pub trait MapInsertShim<K: Borrow<Q>, Q: ?Sized, V> : MapReadShim<Q, V> + Clearable {
    fn insert(&mut self, key: K, value: V) -> Option<V>;
}

pub trait MapShim<K: Borrow<Q>, Q: ?Sized, V> : MapInsertShim<K, Q, V> + MapReadShim<Q, V> + Clearable {
    fn get_mut(&mut self, key: &Q) -> Option<&mut V>;
    fn remove(&mut self, key: &Q) -> Option<V>;
}

pub trait MapRefKeyShim<Q: ?Sized, V> : MapRefKeyInsertShim<Q, V> + MapReadShim<Q, V> + Clearable {
    fn get_mut(&mut self, key: &Q) -> Option<&mut V>;
    fn remove(&mut self, key: &Q) -> Option<V>;
}

pub trait Set<T> : Container {
    fn get<Q: ?Sized>(&self, value: &Q) -> Option<&T>
    where
        T: Borrow<Q>,
        Self: SetShim<T, Q> {
        <Self as SetReadShim<T, Q>>::get(self, value)
    }
    fn remove<Q: ?Sized>(&mut self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Self: SetShim<T, Q> {
        <Self as SetShim<T, Q>>::remove(self, value)
    }
    fn insert(&mut self, value: T) -> bool
    where
        Self: SetShim<T, T> {
        <Self as SetShim<T, T>>::insert(self, value)
    }
    fn contains<Q: ?Sized>(&self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Self: SetShim<T, Q> {
        <Self as SetReadShim<T, Q>>::contains(self, value)
    }
    fn union<'a>(&'a self, other: &'a Self) -> <Self as SetReadShim<T, T>>::Union<'a>
    where
        Self: SetShim<T, T> {
        <Self as SetReadShim<T, T>>::union(self, other)
    }
    fn is_disjoint(&self, other: &Self) -> bool
    where
        Self: SetShim<T, T> {
        <Self as SetReadShim<T, T>>::is_disjoint(self, other)
    }
    fn is_subset(&self, other: &Self) -> bool
    where
        Self: SetShim<T, T> {
        <Self as SetReadShim<T, T>>::is_subset(self, other)
    }
    fn is_superset(&self, other: &Self) -> bool
    where
        Self: SetShim<T, T> {
        <Self as SetReadShim<T, T>>::is_superset(self, other)
    }
    fn replace(&mut self, value: T) -> Option<T>
    where
        Self: SetShim<T, T> {
        <Self as SetShim<T, T>>::replace(self, value)
    }
    fn take<Q: ?Sized>(&mut self, value: &Q) -> Option<T>
    where
        T: Borrow<Q>,
        Self: SetShim<T, Q> {
        <Self as SetShim<T, Q>>::take(self, value)
    }
}

pub trait SetReadShim<T: Borrow<Q>, Q: ?Sized> : Container {
    type Union<'a> where Self: 'a;
    fn get(&self, value: &Q) -> Option<&T>;
    fn contains(&self, value: &Q) -> bool;
    fn union<'a>(&'a self, other: &'a Self) -> Self::Union<'a>;
    fn is_disjoint(&self, other: &Self) -> bool;
    fn is_subset(&self, other: &Self) -> bool;
    fn is_superset(&self, other: &Self) -> bool;
}

pub trait SetShim<T: Borrow<Q>, Q: ?Sized> : SetReadShim<T, Q> {
    fn remove(&mut self, value: &Q) -> bool;
    fn insert(&mut self, value: T) -> bool;
    fn replace(&mut self, value: T) -> Option<T>;
    fn take(&mut self, value: &Q) -> Option<T>;
}