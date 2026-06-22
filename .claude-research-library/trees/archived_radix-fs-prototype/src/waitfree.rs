//! **Wait-free write** CoW radix Map (atomics via `arc-swap`, `unsafe`-free).
//!
//! This is the synthesized, adversarially-verified protocol from the design
//! workflow: per-shard **flat combining** (Kogan–Petrank fast-path/slow-path)
//! over the immutable-CoW radix tree, with the two fixes the adversarial review
//! forced before the wait-free claim holds:
//!
//! * **FIX 1 — gate the fast path** (closes the starvation hole). Each shard has
//!   an `AtomicUsize pending` announcement count. A writer takes the bare
//!   fast-path CAS *only while `pending == 0`*; once any writer announces, every
//!   other writer must route through the combiner (which scans all slots). So
//!   the only CASes that can win while a descriptor is announced are combines
//!   that saw it — giving the O(P) inclusion bound. (Without this, an unbounded
//!   stream of bare fast-path CASes can starve an announced writer forever.)
//! * **FIX 2 — seq-stamped, monotone apply** (closes lost-update / double-fold).
//!   Each value is stored as `(op_seq, Value)`; a write applies only if its
//!   `op_seq` exceeds the resident one. Apply is then idempotent and monotone:
//!   re-folding a stale descriptor (or a slow descriptor that lost to a newer
//!   fast write) is a harmless no-op.
//!
//! ## Wait-free bound
//! A write costs `O(K)` fast-path attempts + `O(P)` slow-path help rounds, each
//! round `O(P · KEY_LEN)` work — a hard `O(K + P)` round-trip bound, independent
//! of key count or contention *duration*. No-starvation argument: once writer
//! `w` stores its descriptor, every slot-scan that *starts after* that store
//! includes it; at most `P` combines whose scan started earlier (plus at most
//! `P` fast CASes that passed the gate before the announce) can win without it,
//! after which the next winning combine includes and marks it done. Radix has
//! **no SMOs**, so a combine is a single root CAS with no interleavable sub-CAS
//! sequence — which is exactly why the bound is clean here.
//!
//! ## Honesty
//! Reads stay **wait-free** (atomic load + immutable walk). Writes are
//! wait-free *by the above argument*; the `max_help_rounds` instrumentation lets
//! the stress harness show, empirically, that per-op work stays bounded under
//! adversarial hot-key contention (where the lock-free map's retry tail grows).
//!
//! The gate+combine core is also **loom model-checked** (see
//! `tests/loom_waitfree.rs`): loom exhaustively explores all interleavings of a
//! faithful re-expression of this protocol (it cannot check `arc-swap` directly)
//! and verifies, for N = 2 (exhaustive) and N = 3 (bounded preemptions): the
//! max-seq write always wins (no lost update — FIX 2), every writer completes
//! (no deadlock/livelock), and a slow-path writer commits within `2N+5` rounds
//! (the wait-free bound, machine-checked for small N).

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use arc_swap::{ArcSwap, ArcSwapOption};

use crate::key::KEY_LEN;
use crate::store::Value;

#[derive(Clone)]
struct Node {
    children: Vec<(u8, Arc<Node>)>,
    /// `(op_seq, value)` — seq-stamped for monotone apply (FIX 2).
    entry: Option<(u64, Value)>,
}

impl Node {
    fn empty() -> Self {
        Node {
            children: Vec::new(),
            entry: None,
        }
    }
}

/// Build a fresh chain for a new key suffix.
fn build_chain(suffix: &[u8], seq: u64, value: &Value) -> Node {
    match suffix.split_first() {
        None => Node {
            children: Vec::new(),
            entry: Some((seq, value.clone())),
        },
        Some((b, rest)) => Node {
            children: vec![(*b, Arc::new(build_chain(rest, seq, value)))],
            entry: None,
        },
    }
}

/// Monotone path-copy insert: returns `None` if the write is a no-op because a
/// resident value with `>=` seq already exists (idempotent / superseded).
fn insert_copy(node: &Node, suffix: &[u8], seq: u64, value: &Value) -> Option<Node> {
    match suffix.split_first() {
        None => match &node.entry {
            Some((s, _)) if *s >= seq => None, // superseded -> no-op
            _ => Some(Node {
                children: node.children.clone(),
                entry: Some((seq, value.clone())),
            }),
        },
        Some((b, rest)) => {
            let mut children = node.children.clone();
            match children.binary_search_by_key(b, |(c, _)| *c) {
                Ok(i) => {
                    let nc = insert_copy(&children[i].1, rest, seq, value)?; // propagate no-op
                    children[i] = (*b, Arc::new(nc));
                }
                Err(i) => {
                    children.insert(i, (*b, Arc::new(build_chain(rest, seq, value))));
                }
            }
            Some(Node {
                children,
                entry: node.entry.clone(),
            })
        }
    }
}

/// Path-copy a node with `key` removed; prunes children that become empty.
/// Returns the new node and whether a value was actually present.
fn remove_copy(node: &Node, key: &[u8]) -> (Node, bool) {
    match key.split_first() {
        None => {
            let present = node.entry.is_some();
            (
                Node {
                    children: node.children.clone(),
                    entry: None,
                },
                present,
            )
        }
        Some((b, rest)) => {
            let mut children = node.children.clone();
            let removed = match children.binary_search_by_key(b, |(c, _)| *c) {
                Ok(i) => {
                    let (nc, did) = remove_copy(&children[i].1, rest);
                    if nc.children.is_empty() && nc.entry.is_none() {
                        children.remove(i); // prune emptied subtree
                    } else {
                        children[i] = (*b, Arc::new(nc));
                    }
                    did
                }
                Err(_) => false,
            };
            (
                Node {
                    children,
                    entry: node.entry.clone(),
                },
                removed,
            )
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
    node.entry.as_ref().map(|(_, v)| v)
}

fn collect(node: &Node, path: &mut Vec<u8>, lo: &[u8], hi: &[u8], out: &mut Vec<([u8; KEY_LEN], Value)>) {
    if let Some((_, v)) = &node.entry {
        if path.len() == KEY_LEN && path.as_slice() >= lo && path.as_slice() <= hi {
            let mut k = [0u8; KEY_LEN];
            k.copy_from_slice(path);
            out.push((k, v.clone()));
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

#[inline]
fn shard_index(key: &[u8], n: usize) -> usize {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in key {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (h % n as u64) as usize
}

struct OpDesc {
    key: [u8; KEY_LEN],
    seq: u64,
    value: Value,
    done: AtomicBool,
}

struct Shard {
    root: ArcSwap<Node>,
    announce: Vec<ArcSwapOption<OpDesc>>,
    pending: AtomicUsize,
}

pub struct WaitFreeRadixMap {
    shards: Vec<Shard>,
    max_threads: usize,
    /// Keys are sharded by their first `shard_prefix` bytes. Sharding by a
    /// *prefix* (rather than the whole key) keeps every key that shares that
    /// prefix in ONE shard — so a band/range scan confined to that prefix is a
    /// single-shard, local scan. The FS stack shards by the 8-byte inode, so all
    /// of one inode's keys (every offset, every snapshot) live together and
    /// per-inode reads/scans stay local, while different inodes spread for write
    /// concurrency.
    shard_prefix: usize,
    seq_gen: AtomicU64,
    // instrumentation (the wait-free witness)
    slow_ops: AtomicU64,
    total_help_rounds: AtomicU64,
    max_help_rounds: AtomicU64,
    fast_wins: AtomicU64,
}

/// Fast-path attempts before escalating to the announced slow path.
const K_FAST: usize = 2;

impl WaitFreeRadixMap {
    pub fn new(shard_count: usize, max_threads: usize) -> Self {
        Self::new_with_prefix(shard_count, max_threads, KEY_LEN)
    }

    /// Like `new`, but shard by the first `shard_prefix` key bytes (see
    /// [`WaitFreeRadixMap::shard_prefix`]). `KEY_LEN` = shard by the whole key.
    pub fn new_with_prefix(shard_count: usize, max_threads: usize, shard_prefix: usize) -> Self {
        assert!(shard_count >= 1 && max_threads >= 1 && shard_prefix >= 1);
        let shards = (0..shard_count)
            .map(|_| Shard {
                root: ArcSwap::from_pointee(Node::empty()),
                announce: (0..max_threads).map(|_| ArcSwapOption::empty()).collect(),
                pending: AtomicUsize::new(0),
            })
            .collect();
        WaitFreeRadixMap {
            shards,
            max_threads,
            shard_prefix,
            seq_gen: AtomicU64::new(1),
            slow_ops: AtomicU64::new(0),
            total_help_rounds: AtomicU64::new(0),
            max_help_rounds: AtomicU64::new(0),
            fast_wins: AtomicU64::new(0),
        }
    }

    #[inline]
    fn shard_of(&self, key: &[u8]) -> usize {
        let p = self.shard_prefix.min(key.len());
        shard_index(&key[..p], self.shards.len())
    }

    /// Wait-free write. `tid` is the caller's thread id in `0..max_threads`.
    pub fn put(&self, tid: usize, key: &[u8; KEY_LEN], value: Value) {
        let seq = self.seq_gen.fetch_add(1, Ordering::Relaxed);
        self.put_with_seq(tid, key, value, seq);
    }

    /// Wait-free write with a caller-supplied `seq` — lets an upper layer (the
    /// full FS stack) use one op-sequence for both the map's monotone apply and
    /// the durability journal, so journal replay reconstructs the same state.
    pub fn put_with_seq(&self, tid: usize, key: &[u8; KEY_LEN], value: Value, seq: u64) {
        assert!(tid < self.max_threads, "tid out of range");
        let sh = &self.shards[self.shard_of(key)];

        // FAST PATH (gated by `pending`, FIX 1).
        let mut attempt = 0;
        while attempt < K_FAST && sh.pending.load(Ordering::Acquire) == 0 {
            attempt += 1;
            let cur = sh.root.load_full();
            match insert_copy(&cur, key, seq, &value) {
                None => {
                    self.fast_wins.fetch_add(1, Ordering::Relaxed);
                    return; // superseded by a newer write -> done
                }
                Some(n) => {
                    let new = Arc::new(n);
                    let prev = sh.root.compare_and_swap(&cur, new);
                    if Arc::ptr_eq(&prev, &cur) {
                        self.fast_wins.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                }
            }
        }

        // SLOW PATH: announce, then help-combine until done.
        let d = Arc::new(OpDesc {
            key: *key,
            seq,
            value,
            done: AtomicBool::new(false),
        });
        sh.announce[tid].store(Some(d.clone()));
        sh.pending.fetch_add(1, Ordering::AcqRel);

        let mut rounds = 0u64;
        loop {
            self.help(sh);
            rounds += 1;
            if d.done.load(Ordering::Acquire) {
                break;
            }
        }

        sh.announce[tid].store(None);
        sh.pending.fetch_sub(1, Ordering::AcqRel);

        self.slow_ops.fetch_add(1, Ordering::Relaxed);
        self.total_help_rounds.fetch_add(rounds, Ordering::Relaxed);
        self.max_help_rounds.fetch_max(rounds, Ordering::Relaxed);
    }

    /// One combine round: fold every announced, not-done descriptor into a single
    /// new root and publish it with one CAS; on success, mark them all done.
    fn help(&self, sh: &Shard) {
        let cur = sh.root.load_full();
        let mut batch: Vec<Arc<OpDesc>> = Vec::new();
        for slot in &sh.announce {
            if let Some(d) = slot.load_full() {
                if !d.done.load(Ordering::Acquire) {
                    batch.push(d);
                }
            }
        }
        if batch.is_empty() {
            return;
        }
        let mut acc = (*cur).clone();
        let mut changed = false;
        for d in &batch {
            if let Some(n) = insert_copy(&acc, &d.key, d.seq, &d.value) {
                acc = n;
                changed = true;
            }
        }
        let new = if changed { Arc::new(acc) } else { Arc::clone(&cur) };
        let prev = sh.root.compare_and_swap(&cur, new);
        if Arc::ptr_eq(&prev, &cur) {
            for d in &batch {
                d.done.store(true, Ordering::Release);
            }
        }
    }

    /// Lock-free physical removal of a key (CoW path-copy, prunes emptied
    /// nodes). Used by snapshot GC to reclaim a dead snapshot's versions; not on
    /// the hot path, so lock-free (rcu) rather than wait-free. Returns whether a
    /// value was present.
    pub fn remove(&self, key: &[u8; KEY_LEN]) -> bool {
        let s = &self.shards[self.shard_of(key)];
        let mut removed = false;
        s.root.rcu(|cur| {
            let (n, did) = remove_copy(cur, key);
            removed = did;
            Arc::new(n)
        });
        removed
    }

    /// Wait-free point read.
    pub fn get(&self, key: &[u8; KEY_LEN]) -> Option<Value> {
        let root = self.shards[self.shard_of(key)].load_root();
        get_node(&root, key).cloned()
    }

    /// Ordered range scan over `[lo, hi]`. When `lo` and `hi` share the shard
    /// prefix (the common per-inode case), this is a **single-shard, local**
    /// scan; otherwise it merges across all shards.
    pub fn range_inclusive(&self, lo: &[u8; KEY_LEN], hi: &[u8; KEY_LEN]) -> Vec<([u8; KEY_LEN], Value)> {
        let mut out = Vec::new();
        let p = self.shard_prefix.min(KEY_LEN);
        if lo[..p] == hi[..p] {
            // Same shard prefix -> one shard holds the whole range.
            let root = self.shards[self.shard_of(lo)].load_root();
            let mut path = Vec::with_capacity(KEY_LEN);
            collect(&root, &mut path, lo, hi, &mut out);
        } else {
            for sh in &self.shards {
                let root = sh.load_root();
                let mut path = Vec::with_capacity(KEY_LEN);
                collect(&root, &mut path, lo, hi, &mut out);
            }
        }
        out.sort_by_key(|e| e.0);
        out
    }

    /// O(shards) immutable snapshot (same CoW mechanism as the lock-free map).
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            roots: self.shards.iter().map(|sh| sh.load_root()).collect(),
        }
    }

    // ---- instrumentation accessors (the wait-free witness) ----
    pub fn slow_ops(&self) -> u64 {
        self.slow_ops.load(Ordering::Relaxed)
    }
    pub fn fast_wins(&self) -> u64 {
        self.fast_wins.load(Ordering::Relaxed)
    }
    pub fn max_help_rounds(&self) -> u64 {
        self.max_help_rounds.load(Ordering::Relaxed)
    }
    pub fn total_help_rounds(&self) -> u64 {
        self.total_help_rounds.load(Ordering::Relaxed)
    }
}

impl Shard {
    fn load_root(&self) -> Arc<Node> {
        self.root.load_full()
    }
}

/// An immutable, isolated point-in-time view.
pub struct Snapshot {
    roots: Vec<Arc<Node>>,
}

impl Snapshot {
    pub fn get(&self, key: &[u8; KEY_LEN]) -> Option<Value> {
        let root = &self.roots[shard_index(key, self.roots.len())];
        get_node(root, key).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::encode;

    #[test]
    fn single_thread_basics() {
        let m = WaitFreeRadixMap::new(8, 4);
        m.put(0, &encode(1, 0, 1), Value::Inode(10));
        m.put(0, &encode(1, 1, 1), Value::Inode(11));
        m.put(0, &encode(1, 0, 1), Value::Inode(99)); // later seq overwrites
        assert_eq!(m.get(&encode(1, 0, 1)), Some(Value::Inode(99)));
        assert_eq!(m.get(&encode(1, 1, 1)), Some(Value::Inode(11)));
        assert_eq!(m.get(&encode(2, 0, 1)), None);
    }

    #[test]
    fn ordered_range() {
        let m = WaitFreeRadixMap::new(16, 2);
        for off in [50u64, 10, 30, 20, 40] {
            m.put(0, &encode(1, off, 1), Value::Inode(off));
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
}
