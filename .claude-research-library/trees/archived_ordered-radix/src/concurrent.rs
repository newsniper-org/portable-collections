//! `ConcurrentRadixMap` — the lock-free concurrent CoW radix map (`--features
//! concurrent`). Same immutable-node CoW structure as [`crate::CowRadixMap`],
//! but the root is an atomic `Arc` (`arc-swap`) so:
//!
//! * **reads are wait-free** (atomic `Arc` load + immutable walk),
//! * **writes are lock-free** (path-copy + atomic root swap; retry only on a
//!   same-shard race),
//! * **reclamation is `Arc` refcounting** — no epochs / hazard pointers / `unsafe`,
//! * **snapshots are O(shards)** (capture each shard root).
//!
//! Keys shard by a configurable prefix length: sharding by a *prefix* keeps
//! every key with that prefix in one shard, so a range confined to the prefix
//! is a single-shard local scan (the FS shards by the inode prefix). A
//! genuinely wait-free *write* path (per-shard flat combining) lives in the
//! source prototype; this crate ships the lock-free baseline, which is the right
//! fit for an in-DRAM metadata cache.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use arc_swap::ArcSwap;

struct Node<V> {
    children: Vec<(u8, Arc<Node<V>>)>,
    value: Option<V>,
}

impl<V: Clone> Clone for Node<V> {
    fn clone(&self) -> Self {
        Node {
            children: self.children.clone(),
            value: self.value.clone(),
        }
    }
}

impl<V> Node<V> {
    fn empty() -> Self {
        Node {
            children: Vec::new(),
            value: None,
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

fn insert_copy<V: Clone>(node: &Node<V>, suffix: &[u8], value: V) -> Node<V> {
    match suffix.split_first() {
        None => Node {
            children: node.children.clone(),
            value: Some(value),
        },
        Some((b, rest)) => {
            let mut children = node.children.clone();
            match children.binary_search_by_key(b, |(c, _)| *c) {
                Ok(i) => children[i] = (*b, Arc::new(insert_copy(&children[i].1, rest, value))),
                Err(i) => children.insert(i, (*b, Arc::new(build_chain(rest, value)))),
            }
            Node {
                children,
                value: node.value.clone(),
            }
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
    if let Some(v) = &node.value {
        if path.as_slice() >= lo && path.as_slice() <= hi {
            out.push((path.clone(), v.clone()));
        }
    }
    for (b, child) in &node.children {
        path.push(*b);
        let len = path.len();
        if !(path.as_slice() < &lo[..len.min(lo.len())] || path.as_slice() > &hi[..len.min(hi.len())]) {
            collect(child, path, lo, hi, out);
        }
        path.pop();
    }
}

fn shard_index(key: &[u8], n: usize) -> usize {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in key {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (h % n as u64) as usize
}

/// Lock-free concurrent ordered radix map.
pub struct ConcurrentRadixMap<V> {
    shards: Vec<ArcSwap<Node<V>>>,
    shard_prefix: usize,
    retries: AtomicU64,
}

impl<V: Clone> ConcurrentRadixMap<V> {
    /// `shard_prefix` = number of leading key bytes used to pick a shard.
    /// Use the key length for whole-key sharding, or a logical prefix (e.g. the
    /// 8-byte inode) to keep one object's keys in one shard for local scans.
    pub fn new(shard_count: usize, shard_prefix: usize) -> Self {
        assert!(shard_count >= 1 && shard_prefix >= 1);
        ConcurrentRadixMap {
            shards: (0..shard_count).map(|_| ArcSwap::from_pointee(Node::empty())).collect(),
            shard_prefix,
            retries: AtomicU64::new(0),
        }
    }

    fn shard_of(&self, key: &[u8]) -> usize {
        let p = self.shard_prefix.min(key.len());
        shard_index(&key[..p], self.shards.len())
    }

    /// Lock-free insert / overwrite.
    pub fn insert(&self, key: &[u8], value: V) {
        let s = &self.shards[self.shard_of(key)];
        let mut tries = 0u64;
        s.rcu(|cur| {
            tries += 1;
            Arc::new(insert_copy(cur, key, value.clone()))
        });
        if tries > 1 {
            self.retries.fetch_add(tries - 1, Ordering::Relaxed);
        }
    }

    /// Wait-free point read.
    pub fn get(&self, key: &[u8]) -> Option<V> {
        let root = self.shards[self.shard_of(key)].load_full();
        get_rec(&root, key).cloned()
    }

    /// Ordered range scan; single-shard when `lo`/`hi` share the shard prefix.
    pub fn range(&self, lo: &[u8], hi: &[u8]) -> Vec<(Vec<u8>, V)> {
        let mut out = Vec::new();
        let p = self.shard_prefix.min(lo.len()).min(hi.len());
        if lo.len() >= p && hi.len() >= p && lo[..p] == hi[..p] {
            let root = self.shards[self.shard_of(lo)].load_full();
            collect(&root, &mut Vec::new(), lo, hi, &mut out);
        } else {
            for sh in &self.shards {
                let root = sh.load_full();
                collect(&root, &mut Vec::new(), lo, hi, &mut out);
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// O(shards) immutable snapshot.
    pub fn snapshot(&self) -> Snapshot<V> {
        Snapshot {
            roots: self.shards.iter().map(|s| s.load_full()).collect(),
            shard_prefix: self.shard_prefix,
        }
    }

    pub fn retries(&self) -> u64 {
        self.retries.load(Ordering::Relaxed)
    }
}

/// An immutable, isolated point-in-time view of a [`ConcurrentRadixMap`].
pub struct Snapshot<V> {
    roots: Vec<Arc<Node<V>>>,
    shard_prefix: usize,
}

impl<V: Clone> Snapshot<V> {
    pub fn get(&self, key: &[u8]) -> Option<V> {
        let p = self.shard_prefix.min(key.len());
        let idx = shard_index(&key[..p], self.roots.len());
        get_rec(&self.roots[idx], key).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basics_and_snapshot() {
        let m: ConcurrentRadixMap<u32> = ConcurrentRadixMap::new(8, 1);
        m.insert(b"k", 1);
        let snap = m.snapshot();
        m.insert(b"k", 2);
        assert_eq!(snap.get(b"k"), Some(1));
        assert_eq!(m.get(b"k"), Some(2));
    }

    #[test]
    fn ordered_range_same_prefix_single_shard() {
        let m: ConcurrentRadixMap<u32> = ConcurrentRadixMap::new(16, 1);
        for (k, v) in [(b"i5", 5u32), (b"i1", 1), (b"i3", 3)] {
            m.insert(k, v);
        }
        let got: Vec<u32> = m.range(b"i0", b"i9").into_iter().map(|(_, v)| v).collect();
        assert_eq!(got, alloc::vec![1, 3, 5]);
    }
}
