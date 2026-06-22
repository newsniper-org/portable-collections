//! `ArtCowMap` — a path-compressed **Adaptive Radix Tree** with copy-on-write
//! and O(1) snapshots. `no_std`, `unsafe`-free.
//!
//! The backbone-decision experiment requested by `filesystem-researches`: close
//! the naive byte-radix's depth/memory gap with the two techniques their key
//! entropy demands — **path compression** (lazy-expansion leaves + a per-inner
//! compressed prefix) collapses the shared 8-byte inode prefix to ~1 node, and
//! **adaptive nodes** (Node4 → Node16 → Node48 → Node256) keep the random `h64`
//! fan-out shallow-and-wide instead of one node per byte — while preserving the
//! CoW O(1) snapshot (root `Arc` clone) that is the whole reason to use radix.
//!
//! Node-type growth is a **node replacement** (a new larger
//! node is built and the parent is path-copied to point at it), never an in-place
//! mutation — so the structure stays SMO-free and the concurrent variant's
//! writes stay lock-free / reads wait-free.
//!
//! Keys must be **non-prefix-free** (no key a prefix of another) — fixed-width
//! keys, as the FS uses, satisfy this. Values are `V: Clone`.

use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::traits::{OrderedMap, SnapshotMap};

enum Node<V> {
    Leaf { key: Vec<u8>, value: V },
    Inner { prefix: Vec<u8>, children: Children<V> },
}

/// Adaptive child container. N4/N16 are sorted `(byte -> child)` arrays; N48 is a
/// 256-entry index into a dense slot vec; N256 is a direct 256-way array.
enum Children<V> {
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
fn insert_rec_with<V: Clone, R: Fn(&V, &V) -> bool>(
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

fn get_rec<'a, V>(node: &'a Node<V>, key: &[u8], depth: usize) -> Option<&'a V> {
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

fn collect<V: Clone>(node: &Node<V>, path: &mut Vec<u8>, lo: &[u8], hi: &[u8], out: &mut Vec<(Vec<u8>, V)>) {
    match node {
        Node::Leaf { key, value } => {
            if key.as_slice() >= lo && key.as_slice() <= hi {
                out.push((key.clone(), value.clone()));
            }
        }
        Node::Inner { prefix, children } => {
            let base = path.len();
            path.extend_from_slice(prefix);
            children.for_each(|byte, child| {
                path.push(byte);
                let len = path.len();
                if !(path.as_slice() < &lo[..len.min(lo.len())] || path.as_slice() > &hi[..len.min(hi.len())]) {
                    collect(child, path, lo, hi, out);
                }
                path.pop();
            });
            path.truncate(base);
        }
    }
}

/// Path-compressed adaptive radix tree (CoW, O(1) snapshots).
pub struct ArtCowMap<V> {
    root: Option<Arc<Node<V>>>,
    len: usize,
}

impl<V: Clone> Default for ArtCowMap<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: Clone> ArtCowMap<V> {
    pub fn new() -> Self {
        ArtCowMap { root: None, len: 0 }
    }

    /// Non-allocating ordered range visitor.
    pub fn for_each_range<F: FnMut(&[u8], &V)>(&self, lo: &[u8], hi: &[u8], mut f: F) {
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

    // ---- metrics (the backbone-decision criteria) ----

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

impl<V: Clone> OrderedMap<V> for ArtCowMap<V> {
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

    fn get(&self, key: &[u8]) -> Option<&V> {
        self.root.as_ref().and_then(|r| get_rec(r, key, 0))
    }

    fn range(&self, lo: &[u8], hi: &[u8]) -> Vec<(Vec<u8>, V)> {
        let mut out = Vec::new();
        if let Some(r) = &self.root {
            let mut path = Vec::new();
            collect(r, &mut path, lo, hi, &mut out);
        }
        out
    }

    fn len(&self) -> usize {
        self.len
    }
}

impl<V: Clone> SnapshotMap<V> for ArtCowMap<V> {
    type Snapshot = ArtCowMap<V>;
    /// O(1): clone the root `Arc`.
    fn snapshot(&self) -> Self::Snapshot {
        ArtCowMap {
            root: self.root.clone(),
            len: self.len,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;

    fn k(a: u64, b: u64) -> [u8; 16] {
        let mut x = [0u8; 16];
        x[0..8].copy_from_slice(&a.to_be_bytes());
        x[8..16].copy_from_slice(&b.to_be_bytes());
        x
    }

    #[test]
    fn basics() {
        let mut m: ArtCowMap<u32> = ArtCowMap::new();
        assert_eq!(m.insert(&k(1, 1), 10), None);
        assert_eq!(m.insert(&k(1, 2), 20), None);
        assert_eq!(m.insert(&k(1, 1), 99), Some(10));
        assert_eq!(m.get(&k(1, 1)), Some(&99));
        assert_eq!(m.get(&k(9, 9)), None);
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn snapshot_isolated() {
        let mut m: ArtCowMap<u32> = ArtCowMap::new();
        m.insert(&k(1, 1), 1);
        let s = m.snapshot();
        m.insert(&k(1, 1), 2);
        m.insert(&k(2, 2), 3);
        assert_eq!(s.get(&k(1, 1)), Some(&1));
        assert_eq!(s.get(&k(2, 2)), None);
        assert_eq!(m.get(&k(1, 1)), Some(&2));
    }

    #[test]
    fn differential_vs_btreemap_with_node_growth() {
        // Insert enough keys under one inode to force N4->N16->N48->N256 growth.
        let mut art: ArtCowMap<u64> = ArtCowMap::new();
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
        assert_eq!(art.len(), bt.len());
        // range (one inode) matches BTreeMap
        let lo = k(2, 0);
        let hi = k(2, u64::MAX);
        let mut a: Vec<u64> = art.range(&lo, &hi).into_iter().map(|(_, v)| v).collect();
        let mut b: Vec<u64> = bt.range(lo..=hi).map(|(_, v)| *v).collect();
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, b, "range mismatch");
        // node growth actually happened
        let h = art.node_type_histogram();
        assert!(h[3] > 0 || h[2] > 0, "expected wide nodes (N48/N256), got {h:?}");
    }
}

// ---- concurrent, seq-stamped ART backbone (lock-free) ----
// Closes criterion (B)6 (lock-free node-type growth) AND the LVIAARC interface:
// values carry an `op_seq` for monotone apply (FIX2), plus a public batch-apply
// (sorted ops -> one root transition per shard) and per-key / integrated
// generation queries.
#[cfg(feature = "concurrent")]
pub use conc::{ArtSnapshot, ConcurrentArt};

#[cfg(feature = "concurrent")]
mod conc {
    use super::{get_rec, insert_rec_with, Node};
    use alloc::sync::Arc;
    use alloc::vec::Vec;
    use core::sync::atomic::{AtomicU64, Ordering};

    use arc_swap::ArcSwapOption;

    // Each value is `(op_seq, V)`; apply is monotone (a write lands only if its
    // op_seq exceeds the resident one) — this is the FS prototype's FIX2.
    type Root<V> = Option<Arc<Node<(u64, V)>>>;
    type Slot<V> = ArcSwapOption<Node<(u64, V)>>;

    fn shard_of(key: &[u8], prefix: usize, n: usize) -> usize {
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        for &b in &key[..prefix.min(key.len())] {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        (h % n as u64) as usize
    }

    /// Monotone CoW insert into a (possibly empty) shard root. Returns `None` if
    /// the write is superseded (resident op_seq >= new) — a no-op.
    fn insert_mono<V: Clone>(root: &Root<V>, key: &[u8], value: V, seq: u64) -> Root<V> {
        match root {
            None => Some(Arc::new(Node::Leaf { key: key.to_vec(), value: (seq, value) })),
            Some(r) => insert_rec_with(r, key, 0, (seq, value), &|o: &(u64, V), n: &(u64, V)| n.0 > o.0),
        }
    }

    /// Lock-free, seq-stamped concurrent Adaptive Radix Tree — the LVIAARC
    /// backbone. ART node-type growth (N4→16→48→256) is a pure CoW **node
    /// replacement**, so every write (single or batched) commits as one atomic
    /// root CAS per shard: **wait-free reads, lock-free writes, Arc reclamation,
    /// SMO-free**.
    pub struct ConcurrentArt<V> {
        shards: Vec<Slot<V>>,
        /// Per-shard max applied op_seq — lets recovery bound each shard's scan
        /// independently (a global max can't, if one shard races ahead).
        shard_max: Vec<AtomicU64>,
        prefix: usize,
        seq_gen: AtomicU64,
        max_seq: AtomicU64,
    }

    impl<V: Clone> ConcurrentArt<V> {
        /// `shard_prefix` = leading key bytes used to pick a shard (e.g. 8 = inode).
        pub fn new(shard_count: usize, shard_prefix: usize) -> Self {
            assert!(shard_count >= 1 && shard_prefix >= 1);
            ConcurrentArt {
                shards: (0..shard_count).map(|_| ArcSwapOption::empty()).collect(),
                shard_max: (0..shard_count).map(|_| AtomicU64::new(0)).collect(),
                prefix: shard_prefix,
                seq_gen: AtomicU64::new(1),
                max_seq: AtomicU64::new(0),
            }
        }

        fn slot(&self, key: &[u8]) -> &Slot<V> {
            &self.shards[shard_of(key, self.prefix, self.shards.len())]
        }

        /// Convenience insert (auto op_seq, monotone last-writer-wins).
        pub fn insert(&self, key: &[u8], value: V) {
            let seq = self.seq_gen.fetch_add(1, Ordering::Relaxed);
            self.apply(key, value, seq);
        }

        /// **Apply** one op with a caller-supplied `op_seq` (monotone — FIX2). The
        /// caller (e.g. LVIAARC) owns the op-sequence space; the backbone only
        /// requires it be monotonic per key.
        pub fn apply(&self, key: &[u8], value: V, op_seq: u64) {
            let idx = shard_of(key, self.prefix, self.shards.len());
            self.shards[idx].rcu(|cur| insert_mono(cur, key, value.clone(), op_seq).or_else(|| cur.clone()));
            self.shard_max[idx].fetch_max(op_seq, Ordering::Relaxed);
            self.max_seq.fetch_max(op_seq, Ordering::Relaxed);
        }

        /// **Batch-apply** (the LVIAARC flush primitive): fold a whole `(key,
        /// value, op_seq)` batch into **one root transition per shard** — the
        /// public generalization of the prototype's `help` combine. Atomic
        /// per shard (one CAS), monotone, order-independent.
        pub fn apply_batch(&self, ops: &[(Vec<u8>, V, u64)]) {
            if ops.is_empty() {
                return;
            }
            let n = self.shards.len();
            let mut by_shard: Vec<Vec<usize>> = (0..n).map(|_| Vec::new()).collect();
            let mut shard_maxseq = alloc::vec![0u64; n];
            let mut maxseq = 0u64;
            for (i, (k, _, seq)) in ops.iter().enumerate() {
                let sh = shard_of(k, self.prefix, n);
                by_shard[sh].push(i);
                shard_maxseq[sh] = shard_maxseq[sh].max(*seq);
                maxseq = maxseq.max(*seq);
            }
            for (sh, idxs) in by_shard.iter().enumerate() {
                if idxs.is_empty() {
                    continue;
                }
                self.shards[sh].rcu(|cur| {
                    let mut root = cur.clone();
                    for &i in idxs {
                        let (k, v, seq) = &ops[i];
                        if let Some(nr) = insert_mono(&root, k, v.clone(), *seq) {
                            root = Some(nr);
                        }
                    }
                    root
                });
                self.shard_max[sh].fetch_max(shard_maxseq[sh], Ordering::Relaxed);
            }
            self.max_seq.fetch_max(maxseq, Ordering::Relaxed);
        }

        /// Wait-free point read.
        pub fn get(&self, key: &[u8]) -> Option<V> {
            self.slot(key).load_full().and_then(|r| get_rec(&r, key, 0).map(|(_, v)| v.clone()))
        }

        /// Per-key **integrated generation**: the `op_seq` under which `key` is in
        /// the backbone (LVIAARC's recovery dominance query). `None` if absent.
        pub fn key_seq(&self, key: &[u8]) -> Option<u64> {
            self.slot(key).load_full().and_then(|r| get_rec(&r, key, 0).map(|(s, _)| *s))
        }

        /// Coarse integrated generation: max `op_seq` ever applied (fast-path
        /// "is everything up to seq S already in the backbone?").
        pub fn integrated_generation(&self) -> u64 {
            self.max_seq.load(Ordering::Relaxed)
        }

        /// Number of shards (recovery iterates these).
        pub fn num_shards(&self) -> usize {
            self.shards.len()
        }

        /// The shard a key maps to (inode-prefix hash) — so the cache can group
        /// its own ops per shard for `apply_batch` / recovery.
        pub fn shard_index(&self, key: &[u8]) -> usize {
            shard_of(key, self.prefix, self.shards.len())
        }

        /// **Per-shard** max applied op_seq — recovery scans shard `s` need only
        /// reconcile cached ops with seq > `shard_max_seq(s)` (a global max
        /// over-scans when one shard races ahead). Low-priority companion to
        /// `key_seq` / `integrated_generation`.
        pub fn shard_max_seq(&self, shard: usize) -> u64 {
            self.shard_max[shard].load(Ordering::Relaxed)
        }

        /// O(shards) immutable snapshot.
        pub fn snapshot(&self) -> ArtSnapshot<V> {
            ArtSnapshot {
                roots: self.shards.iter().map(|s| s.load_full()).collect(),
                prefix: self.prefix,
            }
        }
    }

    pub struct ArtSnapshot<V> {
        roots: Vec<Root<V>>,
        prefix: usize,
    }

    impl<V: Clone> ArtSnapshot<V> {
        pub fn get(&self, key: &[u8]) -> Option<V> {
            let idx = shard_of(key, self.prefix, self.roots.len());
            self.roots[idx].as_ref().and_then(|r| get_rec(r, key, 0).map(|(_, v)| v.clone()))
        }
    }
}
