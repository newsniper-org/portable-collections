//! `ShardedArtOrderedMap` — the lock-free, **seq-stamped** concurrent Adaptive
//! Radix Tree (`--features concurrent`). The LVIAARC write-back-cache backbone.
//!
//! Reuses the persistent ART node + recursive helpers from
//! [`super::art`] (`Node`, `get_rec`, `insert_rec_with`), wrapping per-shard
//! roots in an atomic `Arc` (`arc-swap`). Each value carries an `op_seq` and
//! apply is **monotone** (a write lands only if its `op_seq` exceeds the resident
//! one) — the FS prototype's FIX2. ART node-type growth (N4→16→48→256) is a pure
//! CoW node replacement, so every write (single or batched) commits as one
//! atomic root CAS per shard: **wait-free reads, lock-free writes, Arc
//! reclamation, SMO-free**.
//!
//! `concurrent` needs only `alloc` (`arc-swap` is `no_std`-capable); this module
//! compiles on the `no_std` + `alloc` tier (threaded tests sit behind `concurrent-std`).

use super::art::{get_rec, insert_rec_with, Node};
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

/// Lock-free, seq-stamped concurrent Adaptive Radix Tree — the LVIAARC backbone.
pub struct ShardedArtOrderedMap<V> {
    shards: Vec<Slot<V>>,
    /// Per-shard max applied op_seq — lets recovery bound each shard's scan
    /// independently (a global max can't, if one shard races ahead).
    shard_max: Vec<AtomicU64>,
    prefix: usize,
    seq_gen: AtomicU64,
    max_seq: AtomicU64,
}

impl<V: Clone> ShardedArtOrderedMap<V> {
    /// `shard_prefix` = leading key bytes used to pick a shard (e.g. 8 = inode).
    #[must_use]
    pub fn new(shard_count: usize, shard_prefix: usize) -> Self {
        assert!(shard_count >= 1 && shard_prefix >= 1);
        ShardedArtOrderedMap {
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
    #[must_use]
    pub fn get(&self, key: &[u8]) -> Option<V> {
        self.slot(key).load_full().and_then(|r| get_rec(&r, key, 0).map(|(_, v)| v.clone()))
    }

    /// Per-key **integrated generation**: the `op_seq` under which `key` is in
    /// the backbone (LVIAARC's recovery dominance query). `None` if absent.
    #[must_use]
    pub fn key_seq(&self, key: &[u8]) -> Option<u64> {
        self.slot(key).load_full().and_then(|r| get_rec(&r, key, 0).map(|(s, _)| *s))
    }

    /// Coarse integrated generation: max `op_seq` ever applied (fast-path
    /// "is everything up to seq S already in the backbone?").
    #[must_use]
    pub fn integrated_generation(&self) -> u64 {
        self.max_seq.load(Ordering::Relaxed)
    }

    /// Number of shards (recovery iterates these).
    #[must_use]
    pub fn num_shards(&self) -> usize {
        self.shards.len()
    }

    /// The shard a key maps to (inode-prefix hash) — so the cache can group
    /// its own ops per shard for `apply_batch` / recovery.
    #[must_use]
    pub fn shard_index(&self, key: &[u8]) -> usize {
        shard_of(key, self.prefix, self.shards.len())
    }

    /// **Per-shard** max applied op_seq — recovery scans shard `s` need only
    /// reconcile cached ops with seq > `shard_max_seq(s)` (a global max
    /// over-scans when one shard races ahead). Low-priority companion to
    /// `key_seq` / `integrated_generation`.
    #[must_use]
    pub fn shard_max_seq(&self, shard: usize) -> u64 {
        self.shard_max[shard].load(Ordering::Relaxed)
    }

    /// O(shards) immutable snapshot.
    #[must_use]
    pub fn snapshot(&self) -> ShardedArtSnapshot<V> {
        ShardedArtSnapshot {
            roots: self.shards.iter().map(|s| s.load_full()).collect(),
            prefix: self.prefix,
        }
    }
}

/// An immutable, isolated point-in-time view of a [`ShardedArtOrderedMap`].
pub struct ShardedArtSnapshot<V> {
    roots: Vec<Root<V>>,
    prefix: usize,
}

impl<V: Clone> ShardedArtSnapshot<V> {
    #[must_use]
    pub fn get(&self, key: &[u8]) -> Option<V> {
        let idx = shard_of(key, self.prefix, self.roots.len());
        self.roots[idx].as_ref().and_then(|r| get_rec(r, key, 0).map(|(_, v)| v.clone()))
    }
}
