//! `ArtOrderedMap` — a path-compressed **Adaptive Radix Tree** with copy-on-write
//! and O(1) snapshots. `no_std`, `unsafe`-free.
//!
//! Closes the naive byte-radix's depth/memory gap with the two techniques the
//! key entropy demands — **path compression** (lazy-expansion leaves + a
//! per-inner compressed prefix) collapses the shared 8-byte inode prefix to ~1
//! node, and **adaptive nodes** (Node4 → Node16 → Node48 → Node256) keep the
//! random `h64` fan-out shallow-and-wide instead of one node per byte — while
//! preserving the CoW O(1) snapshot (root `Arc` clone) that is the whole reason
//! to use radix.
//!
//! Node-type growth is a **node replacement** (a new larger node is built and
//! the parent is path-copied to point at it), never an in-place mutation — so
//! the structure stays SMO-free and the concurrent variant
//! ([`ShardedArtOrderedMap`](crate::radix::ShardedArtOrderedMap), in
//! `concurrent_art.rs`) keeps lock-free writes / wait-free reads.
//!
//! Keys must be **non-prefix-free** (no key a prefix of another) — fixed-width
//! keys, as the FS uses, satisfy this. Values are `V: Clone`.

use portable_collection_primitives::{MapReadShim, MapRefKeyInsertShim};
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

use portable_collection_primitives::{Container, Clearable};

use portable_collection_primitives::{OrderedMap, SnapshotMap};

// `pub(super)` so the concurrent ART (`concurrent_art.rs`, a sibling module
// under `radix`) can reuse the node type and the two recursive helpers.
pub(super) enum Node<V> {
    Leaf { key: Vec<u8>, value: V },
    Inner { prefix: Vec<u8>, children: Children<V> },
}

/// Adaptive child container. N4/N16 are sorted `(byte -> child)` arrays; N48 is a
/// 256-entry index into a dense slot vec; N256 is a direct 256-way array.
//
// `pub(super)` only because it appears in `Node::Inner`'s field (a `pub(super)`
// enum can't expose a more-private type); nothing outside this module uses it.
pub(super) enum Children<V> {
    N4 { keys: Vec<u8>, kids: Vec<Arc<Node<V>>> },
    N16 { keys: Vec<u8>, kids: Vec<Arc<Node<V>>> },
    N48 { index: Vec<u8>, kids: Vec<Arc<Node<V>>> }, // index[byte]==0 empty else slot+1
    N256 { kids: Vec<Option<Arc<Node<V>>>> },
}

fn common_prefix_len(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

impl<V> Children<V> {
    fn new_pair(b0: u8, c0: Arc<Node<V>>, b1: u8, c1: Arc<Node<V>>) -> Self {
        let (keys, kids) = if b0 < b1 {
            (alloc::vec![b0, b1], alloc::vec![c0, c1])
        } else {
            (alloc::vec![b1, b0], alloc::vec![c1, c0])
        };
        Children::N4 { keys, kids }
    }

    fn get(&self, byte: u8) -> Option<&Arc<Node<V>>> {
        match self {
            Children::N4 { keys, kids } | Children::N16 { keys, kids } => {
                keys.binary_search(&byte).ok().map(|i| &kids[i])
            }
            Children::N48 { index, kids } => {
                let s = index[byte as usize];
                if s == 0 {
                    None
                } else {
                    Some(&kids[(s - 1) as usize])
                }
            }
            Children::N256 { kids } => kids[byte as usize].as_ref(),
        }
    }

    /// Ordered `(byte, &child)` iteration.
    fn for_each<F: FnMut(u8, &Arc<Node<V>>)>(&self, mut f: F) {
        match self {
            Children::N4 { keys, kids } | Children::N16 { keys, kids } => {
                for (k, c) in keys.iter().zip(kids) {
                    f(*k, c);
                }
            }
            Children::N48 { index, kids } => {
                for b in 0..=255u8 {
                    let s = index[b as usize];
                    if s != 0 {
                        f(b, &kids[(s - 1) as usize]);
                    }
                }
            }
            Children::N256 { kids } => {
                for (b, c) in kids.iter().enumerate() {
                    if let Some(c) = c {
                        f(b as u8, c);
                    }
                }
            }
        }
    }

    fn type_tag(&self) -> usize {
        match self {
            Children::N4 { .. } => 0,
            Children::N16 { .. } => 1,
            Children::N48 { .. } => 2,
            Children::N256 { .. } => 3,
        }
    }

    /// The next `(byte, &child)` with `byte >= from`, ascending — the ordered
    /// cursor the lazy range iterator pulls on. `from` is a `u16` so `from ==
    /// 256` cleanly means "past the last byte" (→ `None`).
    fn next_from(&self, from: u16) -> Option<(u8, &Arc<Node<V>>)> {
        match self {
            Children::N4 { keys, kids } | Children::N16 { keys, kids } => {
                let i = keys.partition_point(|&k| u16::from(k) < from);
                keys.get(i).map(|&b| (b, &kids[i]))
            }
            Children::N48 { index, kids } => (from..256).find_map(|b| {
                let s = index[b as usize];
                (s != 0).then(|| (b as u8, &kids[(s - 1) as usize]))
            }),
            Children::N256 { kids } => {
                (from..256).find_map(|b| kids[b as usize].as_ref().map(|c| (b as u8, c)))
            }
        }
    }
}

impl<V: Clone> Children<V> {
    /// CoW: return a copy with `byte` mapped to `child` (byte is NOT already
    /// present), growing the node type if the current one is full.
    fn added(&self, byte: u8, child: Arc<Node<V>>) -> Self {
        match self {
            Children::N4 { keys, kids } => {
                if keys.len() < 4 {
                    let (k, c) = insert_sorted(keys, kids, byte, child);
                    Children::N4 { keys: k, kids: c }
                } else {
                    grow4_to_16(keys, kids).added(byte, child)
                }
            }
            Children::N16 { keys, kids } => {
                if keys.len() < 16 {
                    let (k, c) = insert_sorted(keys, kids, byte, child);
                    Children::N16 { keys: k, kids: c }
                } else {
                    grow16_to_48(keys, kids).added(byte, child)
                }
            }
            Children::N48 { index, kids } => {
                if kids.len() < 48 {
                    let mut index = index.clone();
                    let mut kids = kids.clone();
                    kids.push(child);
                    index[byte as usize] = kids.len() as u8; // slot+1
                    Children::N48 { index, kids }
                } else {
                    grow48_to_256(index, kids).added(byte, child)
                }
            }
            Children::N256 { kids } => {
                let mut kids = kids.clone();
                kids[byte as usize] = Some(child);
                Children::N256 { kids }
            }
        }
    }

    /// CoW: return a copy with `byte` (already present) remapped to `child`.
    fn replaced(&self, byte: u8, child: Arc<Node<V>>) -> Self {
        match self {
            Children::N4 { keys, kids } => {
                let i = keys.binary_search(&byte).unwrap();
                let mut kids = kids.clone();
                kids[i] = child;
                Children::N4 { keys: keys.clone(), kids }
            }
            Children::N16 { keys, kids } => {
                let i = keys.binary_search(&byte).unwrap();
                let mut kids = kids.clone();
                kids[i] = child;
                Children::N16 { keys: keys.clone(), kids }
            }
            Children::N48 { index, kids } => {
                let mut kids = kids.clone();
                kids[(index[byte as usize] - 1) as usize] = child;
                Children::N48 { index: index.clone(), kids }
            }
            Children::N256 { kids } => {
                let mut kids = kids.clone();
                kids[byte as usize] = Some(child);
                Children::N256 { kids }
            }
        }
    }
}

fn insert_sorted<V: Clone>(keys: &[u8], kids: &[Arc<Node<V>>], byte: u8, child: Arc<Node<V>>) -> (Vec<u8>, Vec<Arc<Node<V>>>) {
    let pos = keys.partition_point(|&k| k < byte);
    let mut k = keys.to_vec();
    let mut c = kids.to_vec();
    k.insert(pos, byte);
    c.insert(pos, child);
    (k, c)
}

fn grow4_to_16<V: Clone>(keys: &[u8], kids: &[Arc<Node<V>>]) -> Children<V> {
    Children::N16 { keys: keys.to_vec(), kids: kids.to_vec() }
}

fn grow16_to_48<V: Clone>(keys: &[u8], kids: &[Arc<Node<V>>]) -> Children<V> {
    let mut index = alloc::vec![0u8; 256];
    let mut nkids = Vec::with_capacity(keys.len());
    for (k, c) in keys.iter().zip(kids) {
        nkids.push(c.clone());
        index[*k as usize] = nkids.len() as u8;
    }
    Children::N48 { index, kids: nkids }
}

fn grow48_to_256<V: Clone>(index: &[u8], kids: &[Arc<Node<V>>]) -> Children<V> {
    let mut nkids: Vec<Option<Arc<Node<V>>>> = alloc::vec![None; 256];
    for (b, s) in index.iter().enumerate() {
        if *s != 0 {
            nkids[b] = Some(kids[(*s - 1) as usize].clone());
        }
    }
    Children::N256 { kids: nkids }
}

/// CoW insert with a caller-chosen leaf-replacement policy `replace(existing,
/// new) -> bool`. Returns `None` when the insert is a no-op (existing key whose
/// `replace` says keep) — the basis for monotone (seq-gated) apply. A split for
/// a *new* key always applies.
///
/// `pub(super)` so the concurrent ART can drive a seq-gated `replace`.
pub(super) fn insert_rec_with<V: Clone, R: Fn(&V, &V) -> bool>(
    node: &Arc<Node<V>>,
    key: &[u8],
    depth: usize,
    value: V,
    replace: &R,
) -> Option<Arc<Node<V>>> {
    match &**node {
        Node::Leaf { key: lk, value: lv } => {
            if lk.as_slice() == key {
                return if replace(lv, &value) {
                    Some(Arc::new(Node::Leaf { key: lk.clone(), value }))
                } else {
                    None
                };
            }
            let common = common_prefix_len(&lk[depth..], &key[depth..]);
            let d = depth + common;
            let children = Children::new_pair(
                lk[d],
                node.clone(),
                key[d],
                Arc::new(Node::Leaf { key: key.to_vec(), value }),
            );
            Some(Arc::new(Node::Inner { prefix: lk[depth..d].to_vec(), children }))
        }
        Node::Inner { prefix, children } => {
            let common = common_prefix_len(prefix, &key[depth..]);
            if common < prefix.len() {
                // Prefix mismatch: split this inner node.
                let shortened = Arc::new(Node::Inner {
                    prefix: prefix[common + 1..].to_vec(),
                    children: children_clone(children),
                });
                let new_leaf = Arc::new(Node::Leaf { key: key.to_vec(), value });
                let ch = Children::new_pair(prefix[common], shortened, key[depth + common], new_leaf);
                return Some(Arc::new(Node::Inner { prefix: prefix[..common].to_vec(), children: ch }));
            }
            let d = depth + prefix.len();
            let byte = key[d];
            match children.get(byte) {
                Some(child) => match insert_rec_with(child, key, d + 1, value, replace) {
                    Some(nc) => Some(Arc::new(Node::Inner {
                        prefix: prefix.clone(),
                        children: children.replaced(byte, nc),
                    })),
                    None => None,
                },
                None => {
                    let ch = children.added(byte, Arc::new(Node::Leaf { key: key.to_vec(), value }));
                    Some(Arc::new(Node::Inner { prefix: prefix.clone(), children: ch }))
                }
            }
        }
    }
}

fn insert_rec<V: Clone>(node: &Arc<Node<V>>, key: &[u8], depth: usize, value: V) -> Arc<Node<V>> {
    insert_rec_with(node, key, depth, value, &|_, _| true).expect("plain insert always produces a node")
}

fn children_clone<V: Clone>(c: &Children<V>) -> Children<V> {
    match c {
        Children::N4 { keys, kids } => Children::N4 { keys: keys.clone(), kids: kids.clone() },
        Children::N16 { keys, kids } => Children::N16 { keys: keys.clone(), kids: kids.clone() },
        Children::N48 { index, kids } => Children::N48 { index: index.clone(), kids: kids.clone() },
        Children::N256 { kids } => Children::N256 { kids: kids.clone() },
    }
}

/// `pub(super)` so the concurrent ART can reuse the read path.
pub(super) fn get_rec<'a, V>(node: &'a Node<V>, key: &[u8], depth: usize) -> Option<&'a V> {
    match node {
        Node::Leaf { key: lk, value } => {
            if lk.as_slice() == key {
                Some(value)
            } else {
                None
            }
        }
        Node::Inner { prefix, children } => {
            if key.len() < depth + prefix.len() || &key[depth..depth + prefix.len()] != prefix.as_slice() {
                return None;
            }
            let d = depth + prefix.len();
            children.get(key[d]).and_then(|c| get_rec(c, key, d + 1))
        }
    }
}

/// Path-compressed adaptive radix tree (CoW, O(1) snapshots).
///
/// ```
/// use portable_maps_and_sets::radix::{
///     ArtOrderedMap, MapReadShim, MapRefKeyInsertShim, SnapshotMap,
/// };
///
/// fn k(a: u64, b: u64) -> [u8; 16] {
///     let mut x = [0u8; 16];
///     x[0..8].copy_from_slice(&a.to_be_bytes());
///     x[8..16].copy_from_slice(&b.to_be_bytes());
///     x
/// }
///
/// let mut m: ArtOrderedMap<u32> = ArtOrderedMap::new();
/// m.insert(&k(1, 1), 10);
/// m.insert(&k(1, 2), 20);
/// let snap = m.snapshot();                 // O(1)
/// m.insert(&k(1, 1), 99);
/// assert_eq!(m.get(&k(1, 1)), Some(&99));
/// assert_eq!(snap.get(&k(1, 1)), Some(&10)); // snapshot isolated
/// ```
pub struct ArtOrderedMap<V> {
    root: Option<Arc<Node<V>>>,
    len: usize,
}

impl<V: Clone> Default for ArtOrderedMap<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: Clone> ArtOrderedMap<V> {
    /// Create an empty map. `const` because the empty root is `None` (no
    /// allocation) — unlike [`RadixOrderedMap::new`].
    ///
    /// [`RadixOrderedMap::new`]: crate::radix::RadixOrderedMap::new
    #[must_use]
    pub const fn new() -> Self {
        ArtOrderedMap { root: None, len: 0 }
    }
}

// --- metrics (the backbone-decision criteria; debug/test/bench only) ---
#[doc(hidden)]
impl<V: Clone> ArtOrderedMap<V> {
    pub fn node_count(&self) -> usize {
        fn rec<V>(n: &Node<V>) -> usize {
            match n {
                Node::Leaf { .. } => 1,
                Node::Inner { children, .. } => {
                    let mut c = 1;
                    children.for_each(|_, ch| c += rec(ch));
                    c
                }
            }
        }
        self.root.as_ref().map_or(0, |r| rec(r))
    }

    /// (max leaf depth, avg leaf depth) in node-hops (root inner = depth 1).
    pub fn depth_stats(&self) -> (usize, f64) {
        fn rec<V>(n: &Node<V>, d: usize, max: &mut usize, sum: &mut usize, leaves: &mut usize) {
            match n {
                Node::Leaf { .. } => {
                    *max = (*max).max(d);
                    *sum += d;
                    *leaves += 1;
                }
                Node::Inner { children, .. } => children.for_each(|_, ch| rec(ch, d + 1, max, sum, leaves)),
            }
        }
        let (mut max, mut sum, mut leaves) = (0, 0, 0);
        if let Some(r) = &self.root {
            rec(r, 1, &mut max, &mut sum, &mut leaves);
        }
        (max, if leaves == 0 { 0.0 } else { sum as f64 / leaves as f64 })
    }

    /// Histogram of inner-node types: [N4, N16, N48, N256].
    pub fn node_type_histogram(&self) -> [usize; 4] {
        fn rec<V>(n: &Node<V>, h: &mut [usize; 4]) {
            if let Node::Inner { children, .. } = n {
                h[children.type_tag()] += 1;
                children.for_each(|_, ch| rec(ch, h));
            }
        }
        let mut h = [0; 4];
        if let Some(r) = &self.root {
            rec(r, &mut h);
        }
        h
    }

    /// Rough resident bytes (node structs + child arrays + leaf keys), for a
    /// bytes/key estimate. Approximate — counts capacity by node type.
    pub fn approx_bytes(&self) -> usize {
        let vsz = core::mem::size_of::<V>();
        let psz = core::mem::size_of::<Arc<Node<V>>>(); // pointer slot
        fn rec<V>(n: &Node<V>, vsz: usize, psz: usize) -> usize {
            match n {
                Node::Leaf { key, .. } => 24 + key.len() + vsz,
                Node::Inner { prefix, children } => {
                    let cbytes = match children {
                        Children::N4 { .. } => 4 + 4 * psz,
                        Children::N16 { .. } => 16 + 16 * psz,
                        Children::N48 { .. } => 256 + 48 * psz,
                        Children::N256 { .. } => 256 * psz,
                    };
                    let mut s = 24 + prefix.len() + cbytes;
                    children.for_each(|_, ch| s += rec(ch, vsz, psz));
                    s
                }
            }
        }
        self.root.as_ref().map_or(0, |r| rec(r, vsz, psz))
    }
}

impl<V: Clone> OrderedMap<V> for ArtOrderedMap<V> {
    fn range_ref<'a>(&'a self, lo: &[u8], hi: &[u8]) -> impl Iterator<Item = (Vec<u8>, &'a V)>
    where
        V: 'a,
    {
        ArtRangeRef::new(self.root.as_deref(), lo, hi)
    }

    fn for_each_range<F: FnMut(&[u8], &V)>(&self, lo: &[u8], hi: &[u8], mut f: F) {
        fn rec<V, F: FnMut(&[u8], &V)>(node: &Node<V>, path: &mut Vec<u8>, lo: &[u8], hi: &[u8], f: &mut F) {
            match node {
                Node::Leaf { key, value } => {
                    if key.as_slice() >= lo && key.as_slice() <= hi {
                        f(key, value);
                    }
                }
                Node::Inner { prefix, children } => {
                    let base = path.len();
                    path.extend_from_slice(prefix);
                    children.for_each(|byte, child| {
                        path.push(byte);
                        let len = path.len();
                        if !(path.as_slice() < &lo[..len.min(lo.len())] || path.as_slice() > &hi[..len.min(hi.len())]) {
                            rec(child, path, lo, hi, f);
                        }
                        path.pop();
                    });
                    path.truncate(base);
                }
            }
        }
        if let Some(r) = &self.root {
            let mut path = Vec::new();
            rec(r, &mut path, lo, hi, &mut f);
        }
    }

    // `range` uses the trait default (clones `range_ref`).
}

/// Lazy in-order iterator over an [`ArtOrderedMap`] range `[lo, hi]`, yielding
/// `(owned key, &value)`. Holds borrowed nodes — the tree outlives it via the
/// `&self` borrow that produced it.
pub struct ArtRangeRef<'a, V> {
    lo: Vec<u8>,
    hi: Vec<u8>,
    /// Stack of inner nodes being iterated: `(inner node, next child-byte cursor,
    /// base path length)`. `base` is the path length to truncate to when the node
    /// is exhausted (drops both its compressed prefix and its incoming byte).
    stack: Vec<(&'a Node<V>, u16, usize)>,
    /// The byte path down the stack (prefixes + child bytes).
    path: Vec<u8>,
    /// The root, visited once on the first `next()` (bootstrap).
    pending: Option<&'a Node<V>>,
}

impl<'a, V> ArtRangeRef<'a, V> {
    fn new(root: Option<&'a Node<V>>, lo: &[u8], hi: &[u8]) -> Self {
        ArtRangeRef {
            lo: lo.to_vec(),
            hi: hi.to_vec(),
            stack: Vec::new(),
            path: Vec::new(),
            pending: root,
        }
    }

    /// Descend into `node`; its incoming byte (if any) is already on `path` at
    /// length `fork`. An in-range **leaf** yields its item (and the incoming byte
    /// is dropped, since a leaf gets no frame); an **inner** node pushes a frame
    /// (extending its compressed prefix). Otherwise `None`.
    fn enter(&mut self, node: &'a Node<V>, fork: usize) -> Option<(Vec<u8>, &'a V)> {
        match node {
            Node::Leaf { key, value } => {
                self.path.truncate(fork);
                if key.as_slice() >= self.lo.as_slice() && key.as_slice() <= self.hi.as_slice() {
                    Some((key.clone(), value))
                } else {
                    None
                }
            }
            Node::Inner { prefix, .. } => {
                self.path.extend_from_slice(prefix);
                self.stack.push((node, 0, fork));
                None
            }
        }
    }
}

impl<'a, V> Iterator for ArtRangeRef<'a, V> {
    type Item = (Vec<u8>, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        // Bootstrap: visit the root once.
        if let Some(root) = self.pending.take()
            && let Some(item) = self.enter(root, 0)
        {
            return Some(item);
        }
        loop {
            let depth = self.stack.len();
            if depth == 0 {
                return None;
            }
            let (node, cursor, base) = {
                let top = &self.stack[depth - 1];
                (top.0, top.1, top.2)
            };
            let Node::Inner { children, .. } = node else {
                self.stack.pop(); // defensive: only inner nodes are pushed
                continue;
            };
            if let Some((b, child)) = children.next_from(cursor) {
                self.stack[depth - 1].1 = u16::from(b) + 1;
                let fork = self.path.len();
                self.path.push(b);
                let len = self.path.len();
                let out_of_range = self.path.as_slice() < &self.lo[..len.min(self.lo.len())]
                    || self.path.as_slice() > &self.hi[..len.min(self.hi.len())];
                if out_of_range {
                    self.path.pop();
                    continue;
                }
                let child: &'a Node<V> = child; // &Arc<Node> -> &Node (deref coercion)
                if let Some(item) = self.enter(child, fork) {
                    return Some(item);
                }
            } else {
                self.path.truncate(base);
                self.stack.pop();
            }
        }
    }
}

impl<V: Clone> MapRefKeyInsertShim<[u8], V> for ArtOrderedMap<V> {
    fn insert(&mut self, key: &[u8], value: V) -> Option<V> {
        let old = self.get(key).cloned();
        self.root = Some(match &self.root {
            None => Arc::new(Node::Leaf { key: key.to_vec(), value }),
            Some(r) => insert_rec(r, key, 0, value),
        });
        if old.is_none() {
            self.len += 1;
        }
        old
    }
    
}



impl<V: Clone> MapReadShim<[u8], V> for ArtOrderedMap<V> {
    fn get(&self, key: &[u8]) -> Option<&V> {
        self.root.as_ref().and_then(|r| get_rec(r, key, 0))
    }

    fn contains_key(&self, key: &[u8]) -> bool {
        self.get(key).is_some()
    }
}

impl<V: Clone> SnapshotMap<V> for ArtOrderedMap<V> {
    type Snapshot = ArtOrderedMap<V>;
    /// O(1): clone the root `Arc`.
    fn snapshot(&self) -> Self::Snapshot {
        ArtOrderedMap {
            root: self.root.clone(),
            len: self.len,
        }
    }
}

impl<V: Clone> Container for ArtOrderedMap<V> {
    fn len(&self) -> usize {
        self.len
    }
}

impl<V: Clone> Clearable for ArtOrderedMap<V> {
    /// Reset to empty — writes root and `len` together (the shared invariant).
    fn clear(&mut self) {
        self.root = None;
        self.len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;
    use portable_collection_primitives::{MapReadShim, MapRefKeyInsertShim};

    fn k(a: u64, b: u64) -> [u8; 16] {
        let mut x = [0u8; 16];
        x[0..8].copy_from_slice(&a.to_be_bytes());
        x[8..16].copy_from_slice(&b.to_be_bytes());
        x
    }

    #[test]
    fn basics() {
        let mut m: ArtOrderedMap<u32> = ArtOrderedMap::new();
        assert_eq!(m.insert(&k(1, 1), 10), None);
        assert_eq!(m.insert(&k(1, 2), 20), None);
        assert_eq!(m.insert(&k(1, 1), 99), Some(10));
        assert_eq!(m.get(&k(1, 1)), Some(&99));
        assert_eq!(m.get(&k(9, 9)), None);
        assert_eq!(Container::len(&m), 2);
    }

    #[test]
    fn snapshot_isolated() {
        let mut m: ArtOrderedMap<u32> = ArtOrderedMap::new();
        m.insert(&k(1, 1), 1);
        let s = m.snapshot();
        m.insert(&k(1, 1), 2);
        m.insert(&k(2, 2), 3);
        assert_eq!(s.get(&k(1, 1)), Some(&1));
        assert_eq!(s.get(&k(2, 2)), None);
        assert_eq!(m.get(&k(1, 1)), Some(&2));
    }

    #[test]
    fn container_clear_and_len() {
        let mut m: ArtOrderedMap<u32> = ArtOrderedMap::new();
        m.insert(&k(1, 1), 1);
        m.insert(&k(2, 2), 2);
        assert_eq!(Container::len(&m), 2);
        Clearable::clear(&mut m);
        assert_eq!(Container::len(&m), 0);
        assert!(Container::is_empty(&m));
        assert_eq!(m.get(&k(1, 1)), None);
    }

    #[test]
    fn range_ref_lazy_ordered_and_pruned() {
        let mut m: ArtOrderedMap<u32> = ArtOrderedMap::new();
        // Three inodes; range over the middle inode must skip the others.
        for (a, b, v) in [(1u64, 5u64, 15u32), (2, 9, 29), (2, 1, 21), (2, 7, 27), (3, 0, 30)] {
            m.insert(&k(a, b), v);
        }
        let lo = k(2, 0);
        let hi = k(2, u64::MAX);
        // range_ref: lazy, borrows values, ascending, single-inode after pruning.
        let got: Vec<u32> = m.range_ref(&lo, &hi).map(|(_, v)| *v).collect();
        assert_eq!(got, [21, 27, 29]); // ascending by the second key word
        // lazy first element without materializing the rest.
        assert_eq!(m.range_ref(&lo, &hi).next().map(|(_, v)| *v), Some(21));
        // for_each_range agrees.
        let mut seen = Vec::new();
        m.for_each_range(&lo, &hi, |_, v| seen.push(*v));
        assert_eq!(seen, [21, 27, 29]);
    }

    #[test]
    fn differential_vs_btreemap_with_node_growth() {
        // Insert enough keys under one inode to force N4->N16->N48->N256 growth.
        let mut art: ArtOrderedMap<u64> = ArtOrderedMap::new();
        let mut bt: BTreeMap<[u8; 16], u64> = BTreeMap::new();
        let mut x = 0x1234_5678u64;
        for _ in 0..6000 {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
            let key = k(x % 4, x); // 4 inodes, random second word -> wide fan-out
            art.insert(&key, x);
            bt.insert(key, x);
        }
        // point lookups match
        for (key, v) in &bt {
            assert_eq!(art.get(key), Some(v), "lookup mismatch");
        }
        assert_eq!(Container::len(&art), bt.len());
        // range (one inode) matches BTreeMap
        let lo = k(2, 0);
        let hi = k(2, u64::MAX);
        let mut a: Vec<u64> = art.range(&lo, &hi).map(|(_, v)| v).collect();
        let mut b: Vec<u64> = bt.range(lo..=hi).map(|(_, v)| *v).collect();
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, b, "range mismatch");
        // node growth actually happened
        let h = art.node_type_histogram();
        assert!(h[3] > 0 || h[2] > 0, "expected wide nodes (N48/N256), got {h:?}");
    }
}
