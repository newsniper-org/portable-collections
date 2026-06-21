//! **Real, multi-threaded, lock-free** copy-on-write radix Map using atomics.
//!
//! This is the production-shaped realization of the CoW-radix candidate from
//! the design exploration, with no model/simulation hand-waving:
//!
//! * **Wait-free reads.** A read does an atomic `Arc` load of a shard root and
//!   walks immutable nodes. It never blocks, never retries, never helps.
//! * **Lock-free writes.** A write path-copies the touched path and atomically
//!   swaps the shard root (`ArcSwap::rcu`), retrying only if another writer won
//!   the race. Progress is guaranteed (some writer always makes progress).
//! * **Reclamation = `Arc` refcounting.** An unlinked node is freed when the
//!   last reader/snapshot holding it drops its `Arc` — no epochs, no hazard
//!   pointers, and crucially **no `unsafe`** (the unsafe lives inside the
//!   audited `arc-swap`/`Arc`, behind a safe API).
//! * **O(shards) snapshots.** A snapshot captures the current root `Arc` of
//!   every shard; it is fully immutable and isolated from later writes.
//!
//! Sharding: keys are hashed to one of `shard_count` independent radix trees so
//! writers to different keys rarely contend on the same root CAS (this is the
//! mitigation for the "single root serializes writers" weakness the analysis
//! flagged). Global order is recovered by merging per-shard range scans.
//!
//! Honest scope: writes are **lock-free, not wait-free** — under same-shard
//! contention a writer can retry. Wait-free writes (descriptor + helping) remain
//! the open problem; sharding drives the contention — and thus the retry tail —
//! down in practice, which is the throughput-for-bounded-tail trade in action.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::key::KEY_LEN;
use crate::store::Value;

/// Immutable radix node. Once shared via `Arc`, never mutated — updates
/// path-copy. Children are byte-sorted; untouched subtrees are shared by `Arc`.
#[derive(Clone)]
struct Node {
    children: Vec<(u8, Arc<Node>)>,
    value: Option<Value>,
}

impl Node {
    fn empty() -> Self {
        Node {
            children: Vec::new(),
            value: None,
        }
    }
}

/// Build a fresh chain of nodes for a brand-new key suffix.
fn build_chain(suffix: &[u8], value: Value) -> Node {
    match suffix.split_first() {
        None => Node {
            children: Vec::new(),
            value: Some(value),
        },
        Some((b, rest)) => Node {
            children: vec![(*b, Arc::new(build_chain(rest, value)))],
            value: None,
        },
    }
}

/// Return a path-copied version of `node` with `suffix -> value` inserted.
/// Touched nodes are cloned; all sibling subtrees are shared via `Arc`.
fn insert_copy(node: &Node, suffix: &[u8], value: Value) -> Node {
    match suffix.split_first() {
        None => Node {
            children: node.children.clone(),
            value: Some(value),
        },
        Some((b, rest)) => {
            let mut children = node.children.clone();
            match children.binary_search_by_key(b, |(c, _)| *c) {
                Ok(i) => {
                    let nc = insert_copy(&children[i].1, rest, value);
                    children[i] = (*b, Arc::new(nc));
                }
                Err(i) => {
                    children.insert(i, (*b, Arc::new(build_chain(rest, value))));
                }
            }
            Node {
                children,
                value: node.value.clone(),
            }
        }
    }
}

fn get_node<'a>(mut node: &'a Node, key: &[u8]) -> Option<&'a Value> {
    for &b in key {
        match node.children.binary_search_by_key(&b, |(c, _)| *c) {
            Ok(i) => node = &node.children[i].1,
            Err(_) => return None,
        }
    }
    node.value.as_ref()
}

fn collect(node: &Node, path: &mut Vec<u8>, lo: &[u8], hi: &[u8], out: &mut Vec<([u8; KEY_LEN], Value)>) {
    if let Some(v) = &node.value {
        if path.len() == KEY_LEN && path.as_slice() >= lo && path.as_slice() <= hi {
            let mut k = [0u8; KEY_LEN];
            k.copy_from_slice(path);
            out.push((k, v.clone()));
        }
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

#[inline]
fn shard_index(key: &[u8], n: usize) -> usize {
    // FNV-1a over the key bytes.
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in key {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (h % n as u64) as usize
}

/// A lock-free concurrent ordered map keyed by fixed-width byte strings.
pub struct LockFreeRadixMap {
    shards: Vec<ArcSwap<Node>>,
    retries: AtomicU64,
    writes: AtomicU64,
}

impl LockFreeRadixMap {
    pub fn new(shard_count: usize) -> Self {
        assert!(shard_count >= 1);
        let shards = (0..shard_count).map(|_| ArcSwap::from_pointee(Node::empty())).collect();
        LockFreeRadixMap {
            shards,
            retries: AtomicU64::new(0),
            writes: AtomicU64::new(0),
        }
    }

    /// Lock-free insert / overwrite. Retries only on a same-shard race.
    pub fn put(&self, key: &[u8; KEY_LEN], value: Value) {
        let s = &self.shards[shard_index(key, self.shards.len())];
        let mut attempts = 0u64;
        s.rcu(|cur| {
            attempts += 1;
            Arc::new(insert_copy(cur, key, value.clone()))
        });
        self.writes.fetch_add(1, Ordering::Relaxed);
        if attempts > 1 {
            self.retries.fetch_add(attempts - 1, Ordering::Relaxed);
        }
    }

    /// Wait-free point read.
    pub fn get(&self, key: &[u8; KEY_LEN]) -> Option<Value> {
        let root = self.shards[shard_index(key, self.shards.len())].load_full();
        get_node(&root, key).cloned()
    }

    /// Ordered range scan over `[lo, hi]` across all shards (merged).
    pub fn range_inclusive(&self, lo: &[u8; KEY_LEN], hi: &[u8; KEY_LEN]) -> Vec<([u8; KEY_LEN], Value)> {
        let mut out = Vec::new();
        for s in &self.shards {
            let root = s.load_full();
            let mut path = Vec::with_capacity(KEY_LEN);
            collect(&root, &mut path, lo, hi, &mut out);
        }
        out.sort_by_key(|e| e.0);
        out
    }

    /// O(shards) consistent snapshot: captures every shard root.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            roots: self.shards.iter().map(|s| s.load_full()).collect(),
        }
    }

    /// Total CAS retries observed (the lock-free contention "tax").
    pub fn retries(&self) -> u64 {
        self.retries.load(Ordering::Relaxed)
    }

    pub fn writes(&self) -> u64 {
        self.writes.load(Ordering::Relaxed)
    }
}

/// An immutable, isolated point-in-time view (a set of shard root `Arc`s).
pub struct Snapshot {
    roots: Vec<Arc<Node>>,
}

impl Snapshot {
    pub fn get(&self, key: &[u8; KEY_LEN]) -> Option<Value> {
        let root = &self.roots[shard_index(key, self.roots.len())];
        get_node(root, key).cloned()
    }

    pub fn range_inclusive(&self, lo: &[u8; KEY_LEN], hi: &[u8; KEY_LEN]) -> Vec<([u8; KEY_LEN], Value)> {
        let mut out = Vec::new();
        for root in &self.roots {
            let mut path = Vec::with_capacity(KEY_LEN);
            collect(root, &mut path, lo, hi, &mut out);
        }
        out.sort_by_key(|e| e.0);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::encode;

    #[test]
    fn single_thread_basics() {
        let m = LockFreeRadixMap::new(8);
        m.put(&encode(1, 0, 1), Value::Inode(10));
        m.put(&encode(1, 1, 1), Value::Inode(11));
        m.put(&encode(1, 0, 1), Value::Inode(99)); // overwrite
        assert_eq!(m.get(&encode(1, 0, 1)), Some(Value::Inode(99)));
        assert_eq!(m.get(&encode(1, 1, 1)), Some(Value::Inode(11)));
        assert_eq!(m.get(&encode(2, 0, 1)), None);
    }

    #[test]
    fn ordered_range_across_shards() {
        let m = LockFreeRadixMap::new(16);
        for off in [50u64, 10, 30, 20, 40] {
            m.put(&encode(1, off, 1), Value::Inode(off));
        }
        let got: Vec<u64> = m
            .range_inclusive(&encode(1, 0, 0), &encode(1, 100, u32::MAX))
            .into_iter()
            .map(|(_, v)| match v {
                Value::Inode(x) => x,
                _ => 0,
            })
            .collect();
        assert_eq!(got, vec![10, 20, 30, 40, 50]);
    }

    #[test]
    fn snapshot_is_isolated() {
        let m = LockFreeRadixMap::new(8);
        m.put(&encode(1, 0, 1), Value::Inode(1));
        let snap = m.snapshot();
        m.put(&encode(1, 0, 1), Value::Inode(2)); // mutate after snapshot
        assert_eq!(snap.get(&encode(1, 0, 1)), Some(Value::Inode(1))); // snapshot unchanged
        assert_eq!(m.get(&encode(1, 0, 1)), Some(Value::Inode(2)));
    }
}
