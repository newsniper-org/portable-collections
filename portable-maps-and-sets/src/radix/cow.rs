//! `RadixOrderedMap` — persistent (copy-on-write) ordered byte-radix map.
//!
//! `no_std` + zero-dependency + `unsafe`-free. Nodes are immutable and shared
//! via `Arc`; an insert path-copies the touched path (sharing all untouched
//! subtrees), so a snapshot is just an `Arc` clone of the root — O(1), and fully
//! isolated from later writes (the defining persistent-structure property, and
//! the CoW crash-consistency primitive a filesystem wants).

use portable_collection_primitives::ifstd;

ifstd!({
    use std::sync::Arc;
    use std::vec::Vec;
} else {
    use portable_collection_primitives::ifalloc;
    ifalloc!({
        extern crate alloc;
        use alloc::sync::Arc;
        use alloc::vec::Vec;
    });
});

use portable_collection_primitives::Container;

use super::traits::{OrderedMap, SnapshotMap};

struct Node<V> {
    /// Byte-sorted children (strictly increasing by the `u8`).
    children: Vec<(u8, Arc<Node<V>>)>,
    value: Option<V>,
}

impl<V> Node<V> {
    fn empty() -> Self {
        Node {
            children: Vec::new(),
            value: None,
        }
    }
}

// Manual Clone so we only require `V: Clone` (derive would be fine too).
impl<V: Clone> Clone for Node<V> {
    fn clone(&self) -> Self {
        Node {
            children: self.children.clone(),
            value: self.value.clone(),
        }
    }
}

fn build_chain<V: Clone>(suffix: &[u8], value: V) -> Node<V> {
    match suffix.split_first() {
        None => Node {
            children: Vec::new(),
            value: Some(value),
        },
        Some((b, rest)) => Node {
            children: alloc::vec![(*b, Arc::new(build_chain(rest, value)))],
            value: None,
        },
    }
}

/// Returns a path-copied node with `suffix -> value` set, plus the previous
/// value at that key (if any).
fn insert_rec<V: Clone>(node: &Node<V>, suffix: &[u8], value: V) -> (Node<V>, Option<V>) {
    match suffix.split_first() {
        None => {
            let old = node.value.clone();
            (
                Node {
                    children: node.children.clone(),
                    value: Some(value),
                },
                old,
            )
        }
        Some((b, rest)) => {
            let mut children = node.children.clone();
            let old;
            match children.binary_search_by_key(b, |(c, _)| *c) {
                Ok(i) => {
                    let (nc, o) = insert_rec(&children[i].1, rest, value);
                    children[i] = (*b, Arc::new(nc));
                    old = o;
                }
                Err(i) => {
                    children.insert(i, (*b, Arc::new(build_chain(rest, value))));
                    old = None;
                }
            }
            (
                Node {
                    children,
                    value: node.value.clone(),
                },
                old,
            )
        }
    }
}

fn get_rec<'a, V>(mut node: &'a Node<V>, key: &[u8]) -> Option<&'a V> {
    for &b in key {
        match node.children.binary_search_by_key(&b, |(c, _)| *c) {
            Ok(i) => node = &node.children[i].1,
            Err(_) => return None,
        }
    }
    node.value.as_ref()
}

fn collect<V: Clone>(node: &Node<V>, path: &mut Vec<u8>, lo: &[u8], hi: &[u8], out: &mut Vec<(Vec<u8>, V)>) {
    if let Some(v) = &node.value
        && path.as_slice() >= lo
        && path.as_slice() <= hi
    {
        out.push((path.clone(), v.clone()));
    }
    for (b, child) in &node.children {
        path.push(*b);
        let len = path.len();
        let lop = &lo[..len.min(lo.len())];
        let hip = &hi[..len.min(hi.len())];
        if !(path.as_slice() < lop || path.as_slice() > hip) {
            collect(child, path, lo, hi, out);
        }
        path.pop();
    }
}

fn count<V>(node: &Node<V>) -> usize {
    let mut n = if node.value.is_some() { 1 } else { 0 };
    for (_, c) in &node.children {
        n += count(c);
    }
    n
}

/// Persistent (copy-on-write) ordered radix map. Nodes are heap-allocated and
/// shared behind `Arc`; updates path-copy.
///
/// ```
/// use portable_maps_and_sets::radix::{RadixOrderedMap, OrderedMap, SnapshotMap};
///
/// let mut m: RadixOrderedMap<u32> = RadixOrderedMap::new();
/// assert_eq!(m.insert(b"abc", 1), None);
/// m.insert(b"abd", 2);
/// let snap = m.snapshot();            // O(1), isolated
/// m.insert(b"abc", 9);                // overwrite the live map
/// assert_eq!(m.get(b"abc"), Some(&9));
/// assert_eq!(snap.get(b"abc"), Some(&1)); // snapshot frozen
/// let vals: Vec<u32> = m.range(b"ab", b"abz").into_iter().map(|(_, v)| v).collect();
/// assert_eq!(vals, [9, 2]);           // ascending by key: "abc", "abd"
/// ```
pub struct RadixOrderedMap<V> {
    root: Arc<Node<V>>,
    len: usize,
}

impl<V: Clone> Default for RadixOrderedMap<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: Clone> RadixOrderedMap<V> {
    /// Create an empty map.
    ///
    /// Not a `const fn`: the empty root is an `Arc::new(..)` allocation, and
    /// `Arc::new` is not `const` on stable. (Contrast [`ArtOrderedMap::new`],
    /// whose empty root is `None` and so is `const`.)
    ///
    /// [`ArtOrderedMap::new`]: crate::radix::ArtOrderedMap::new
    #[must_use]
    pub fn new() -> Self {
        RadixOrderedMap {
            root: Arc::new(Node::empty()),
            len: 0,
        }
    }
}

impl<V: Clone> OrderedMap<V> for RadixOrderedMap<V> {
    fn insert(&mut self, key: &[u8], value: V) -> Option<V> {
        let (new_root, old) = insert_rec(&self.root, key, value);
        self.root = Arc::new(new_root);
        if old.is_none() {
            self.len += 1;
        }
        old
    }

    fn get(&self, key: &[u8]) -> Option<&V> {
        get_rec(&self.root, key)
    }

    fn range(&self, lo: &[u8], hi: &[u8]) -> Vec<(Vec<u8>, V)> {
        let mut out = Vec::new();
        let mut path = Vec::new();
        collect(&self.root, &mut path, lo, hi, &mut out);
        out
    }

    fn len(&self) -> usize {
        self.len
    }
}

impl<V: Clone> SnapshotMap<V> for RadixOrderedMap<V> {
    type Snapshot = RadixOrderedMap<V>;

    /// O(1): clone the root `Arc`. The snapshot shares structure with the live
    /// map but is isolated — later inserts path-copy and never mutate shared
    /// nodes.
    fn snapshot(&self) -> Self::Snapshot {
        RadixOrderedMap {
            root: self.root.clone(),
            len: self.len,
        }
    }
}

impl<V: Clone> Container for RadixOrderedMap<V> {
    /// Reset to empty — writes root and `len` together (the shared invariant:
    /// every mutation touches both).
    fn clear(&mut self) {
        self.root = Arc::new(Node::empty());
        self.len = 0;
    }

    fn len(&self) -> usize {
        self.len
    }
}

impl<V> RadixOrderedMap<V> {
    /// Visit every `(key, &value)` in `[lo, hi]` in ascending order, without
    /// allocating a result vector or cloning keys/values (the non-materializing
    /// counterpart to [`OrderedMap::range`]).
    pub fn for_each_range<F: FnMut(&[u8], &V)>(&self, lo: &[u8], hi: &[u8], mut f: F) {
        fn rec<V, F: FnMut(&[u8], &V)>(node: &Node<V>, path: &mut Vec<u8>, lo: &[u8], hi: &[u8], f: &mut F) {
            if let Some(v) = &node.value
                && path.as_slice() >= lo
                && path.as_slice() <= hi
            {
                f(path, v);
            }
            for (b, child) in &node.children {
                path.push(*b);
                let len = path.len();
                if !(path.as_slice() < &lo[..len.min(lo.len())] || path.as_slice() > &hi[..len.min(hi.len())]) {
                    rec(child, path, lo, hi, f);
                }
                path.pop();
            }
        }
        let mut path = Vec::new();
        rec(&self.root, &mut path, lo, hi, &mut f);
    }
}

// --- diagnostics (debug / test / bench only; hidden from the public API docs) ---
#[doc(hidden)]
impl<V: Clone> RadixOrderedMap<V> {
    /// Number of allocated nodes — grows with distinct key bytes, never with
    /// rebalancing (there is none). Memory accounting / tests.
    pub fn node_count(&self) -> usize {
        fn rec<V>(n: &Node<V>) -> usize {
            1 + n.children.iter().map(|(_, c)| rec(c)).sum::<usize>()
        }
        rec(&self.root)
    }

    /// Recount `len` from a fresh walk (validates the cached `len`).
    pub fn recount(&self) -> usize {
        count(&self.root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_overwrite_len() {
        let mut m: RadixOrderedMap<u32> = RadixOrderedMap::new();
        assert_eq!(m.insert(b"abc", 1), None);
        assert_eq!(m.insert(b"abd", 2), None);
        assert_eq!(m.insert(b"abc", 9), Some(1));
        assert_eq!(m.get(b"abc"), Some(&9));
        assert_eq!(m.get(b"abz"), None);
        assert_eq!(OrderedMap::len(&m), 2);
        assert_eq!(m.recount(), 2);
    }

    #[test]
    fn ordered_range() {
        let mut m: RadixOrderedMap<u32> = RadixOrderedMap::new();
        for (k, v) in [(b"50", 50u32), (b"10", 10), (b"30", 30), (b"20", 20)] {
            m.insert(k, v);
        }
        let got: Vec<u32> = m.range(b"00", b"99").into_iter().map(|(_, v)| v).collect();
        assert_eq!(got, [10, 20, 30, 50]);
    }

    #[test]
    fn snapshot_is_isolated_and_o1() {
        let mut m: RadixOrderedMap<u32> = RadixOrderedMap::new();
        m.insert(b"k", 1);
        let snap = m.snapshot(); // O(1)
        m.insert(b"k", 2);
        m.insert(b"new", 3);
        assert_eq!(snap.get(b"k"), Some(&1)); // snapshot unchanged
        assert_eq!(snap.get(b"new"), None);
        assert_eq!(m.get(b"k"), Some(&2));
        assert_eq!(m.get(b"new"), Some(&3));
    }

    #[test]
    fn container_clear_and_len() {
        let mut m: RadixOrderedMap<u32> = RadixOrderedMap::new();
        m.insert(b"a", 1);
        m.insert(b"b", 2);
        assert_eq!(Container::len(&m), 2);
        assert!(!Container::is_empty(&m));
        Container::clear(&mut m);
        assert_eq!(Container::len(&m), 0);
        assert!(Container::is_empty(&m));
        assert_eq!(m.get(b"a"), None);
        assert_eq!(m.recount(), 0);
    }
}
