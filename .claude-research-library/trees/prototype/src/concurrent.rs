//! **The full synthesis stack, in one place.**
//!
//! Composes the verified pieces into the design the exploration arrived at:
//! an ordered, DRAM-authoritative **wait-free radix map** (storage) +
//! **in-key snapshot ids** with ancestry visibility + a **journal** for
//! durability (recovery by replay) + snapshot-consistent range scans.
//!
//! Concurrency guarantees of the whole stack:
//! * reads (`get` / `range`) are **wait-free** — a band scan over the wait-free
//!   map (immutable walk) + a wait-free snapshot-ancestry resolve;
//! * writes (`put` / `delete`) are **wait-free** (the map) + a per-thread
//!   journal append (uncontended) ;
//! * `create_snapshot` is **lock-free** (one `fetch_add` + releases);
//! * recovery (`recover`) replays the merged journal single-threaded.
//!
//! One op-sequence (`seq_gen`) drives both the map's monotone apply and the
//! journal record, so `recover` reconstructs exactly the live state — which the
//! tests check by asserting `recovered.get == live.get` after a concurrent run
//! (this also witnesses linearizability: the concurrent execution equals its own
//! seq-order serialization).
//!
//! Prototype simplifications (documented): the snapshot registry is a fixed-cap
//! atomic array (lock-free create, wait-free query); the per-thread journal uses
//! an uncontended `Mutex<Vec<_>>` per thread (a production system would use a
//! lock-free per-core log). Neither is on the wait-free read path.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;

use crate::key::{decode, encode, Inode, Offset, SnapId};
use crate::store::Value;
use crate::waitfree::WaitFreeRadixMap;

/// Lock-free, fixed-capacity snapshot tree (parent + depth per id). Snapshots
/// are never deleted in this prototype, so the registry only grows.
pub struct ConcSnapshots {
    parent: Vec<AtomicU32>,
    depth: Vec<AtomicU32>,
    next: AtomicU32,
}

impl ConcSnapshots {
    pub fn new(cap: usize) -> Self {
        assert!(cap >= 2, "need room for sentinel + root");
        let parent = (0..cap).map(|_| AtomicU32::new(0)).collect();
        let depth = (0..cap).map(|_| AtomicU32::new(0)).collect();
        // index 0 = sentinel, index 1 = root (parent 0, depth 0).
        ConcSnapshots {
            parent,
            depth,
            next: AtomicU32::new(2),
        }
    }

    #[inline]
    pub fn root(&self) -> SnapId {
        1
    }

    /// Lock-free child creation.
    pub fn add_child(&self, parent: SnapId) -> SnapId {
        let id = self.next.fetch_add(1, Ordering::AcqRel);
        assert!((id as usize) < self.parent.len(), "snapshot capacity exceeded");
        let d = self.depth[parent as usize].load(Ordering::Acquire) + 1;
        self.depth[id as usize].store(d, Ordering::Release);
        self.parent[id as usize].store(parent, Ordering::Release);
        id
    }

    /// Replay path: recreate a child with a specific id.
    pub fn set_child(&self, parent: SnapId, id: SnapId) {
        let d = self.depth[parent as usize].load(Ordering::Acquire) + 1;
        self.depth[id as usize].store(d, Ordering::Release);
        self.parent[id as usize].store(parent, Ordering::Release);
        let mut cur = self.next.load(Ordering::Acquire);
        while cur <= id {
            match self.next.compare_exchange(cur, id + 1, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => break,
                Err(x) => cur = x,
            }
        }
    }

    #[inline]
    pub fn depth(&self, id: SnapId) -> u32 {
        self.depth[id as usize].load(Ordering::Acquire)
    }

    #[inline]
    pub fn count(&self) -> u32 {
        self.next.load(Ordering::Acquire) - 1
    }

    /// Wait-free ancestry query (walk parents, bounded by tree depth).
    pub fn is_ancestor_or_eq(&self, anc: SnapId, mut node: SnapId) -> bool {
        loop {
            if node == anc {
                return true;
            }
            if node == 0 {
                return false;
            }
            node = self.parent[node as usize].load(Ordering::Acquire);
        }
    }
}

/// Resolve the visible version of a key for a read at `read`, given all
/// `(written_snapshot, value)` versions. Tombstone -> absent.
fn resolve(candidates: &[(SnapId, Value)], read: SnapId, snaps: &ConcSnapshots) -> Option<Value> {
    let mut best: Option<(u32, &Value)> = None;
    for (w, v) in candidates {
        if snaps.is_ancestor_or_eq(*w, read) {
            let d = snaps.depth(*w);
            if best.is_none_or(|(bd, _)| d > bd) {
                best = Some((d, v));
            }
        }
    }
    match best {
        Some((_, Value::Tombstone)) | None => None,
        Some((_, v)) => Some(v.clone()),
    }
}

/// A journalled operation (carries its op-sequence for ordered replay).
#[derive(Clone, Debug)]
pub enum ConcOp {
    Put {
        inode: Inode,
        offset: Offset,
        snap: SnapId,
        value: Value,
        seq: u64,
    },
    Snap {
        parent: SnapId,
        child: SnapId,
        seq: u64,
    },
}

impl ConcOp {
    fn seq(&self) -> u64 {
        match self {
            ConcOp::Put { seq, .. } | ConcOp::Snap { seq, .. } => *seq,
        }
    }
}

/// The full concurrent filesystem core: wait-free map + snapshots + journal.
pub struct ConcFs {
    map: WaitFreeRadixMap,
    snaps: ConcSnapshots,
    journal: Vec<Mutex<Vec<ConcOp>>>,
    seq_gen: AtomicU64,
    max_threads: usize,
}

impl ConcFs {
    pub fn new(shards: usize, max_threads: usize, snap_cap: usize) -> Self {
        ConcFs {
            // Shard by the 8-byte inode prefix: all of one inode's keys (every
            // offset + snapshot) live in one shard, so per-inode point reads and
            // range scans are single-shard/local, while different inodes spread
            // across shards for write concurrency.
            map: WaitFreeRadixMap::new_with_prefix(shards, max_threads, 8),
            snaps: ConcSnapshots::new(snap_cap),
            journal: (0..max_threads).map(|_| Mutex::new(Vec::new())).collect(),
            seq_gen: AtomicU64::new(1),
            max_threads,
        }
    }

    pub fn root_snapshot(&self) -> SnapId {
        self.snaps.root()
    }

    pub fn snapshot_count(&self) -> u32 {
        self.snaps.count()
    }

    /// Wait-free write of `(inode, offset)` in snapshot `snap`.
    pub fn put(&self, tid: usize, inode: Inode, offset: Offset, snap: SnapId, value: Value) {
        let seq = self.seq_gen.fetch_add(1, Ordering::Relaxed);
        self.map.put_with_seq(tid, &encode(inode, offset, snap), value.clone(), seq);
        self.journal[tid].lock().unwrap().push(ConcOp::Put { inode, offset, snap, value, seq });
    }

    /// Snapshot-scoped delete (tombstone).
    pub fn delete(&self, tid: usize, inode: Inode, offset: Offset, snap: SnapId) {
        self.put(tid, inode, offset, snap, Value::Tombstone);
    }

    /// Lock-free snapshot creation; returns the new id.
    pub fn create_snapshot(&self, tid: usize, parent: SnapId) -> SnapId {
        let seq = self.seq_gen.fetch_add(1, Ordering::Relaxed);
        let child = self.snaps.add_child(parent);
        self.journal[tid].lock().unwrap().push(ConcOp::Snap { parent, child, seq });
        child
    }

    fn versions(&self, inode: Inode, offset: Offset) -> Vec<(SnapId, Value)> {
        self.map
            .range_inclusive(&encode(inode, offset, 0), &encode(inode, offset, u32::MAX))
            .into_iter()
            .map(|(k, v)| {
                let (_, _, s) = decode(&k);
                (s, v)
            })
            .collect()
    }

    /// Wait-free snapshot-consistent point read.
    pub fn get(&self, inode: Inode, offset: Offset, read: SnapId) -> Option<Value> {
        resolve(&self.versions(inode, offset), read, &self.snaps)
    }

    /// Wait-free snapshot-consistent ordered range scan over `[lo_off, hi_off)`.
    pub fn range(&self, inode: Inode, lo_off: Offset, hi_off: Offset, read: SnapId) -> Vec<(Offset, Value)> {
        let raw = self
            .map
            .range_inclusive(&encode(inode, lo_off, 0), &encode(inode, hi_off, u32::MAX));
        let mut out = Vec::new();
        let mut i = 0;
        while i < raw.len() {
            let (_, cur_off, _) = decode(&raw[i].0);
            if cur_off >= hi_off {
                i += 1;
                continue;
            }
            let mut group = Vec::new();
            while i < raw.len() {
                let (_, off, snap) = decode(&raw[i].0);
                if off != cur_off {
                    break;
                }
                group.push((snap, raw[i].1.clone()));
                i += 1;
            }
            if let Some(v) = resolve(&group, read, &self.snaps) {
                out.push((cur_off, v));
            }
        }
        out
    }

    /// Drain the per-thread journals into one seq-ordered op list (the durable log).
    pub fn drained_ops(&self) -> Vec<ConcOp> {
        let mut ops: Vec<ConcOp> = Vec::new();
        for log in &self.journal {
            ops.extend(log.lock().unwrap().iter().cloned());
        }
        ops.sort_by_key(|o| o.seq());
        ops
    }

    /// Recover a fresh core by replaying a (possibly torn-prefix) journal.
    pub fn recover(ops: &[ConcOp], shards: usize, max_threads: usize, snap_cap: usize) -> ConcFs {
        let fs = ConcFs::new(shards, max_threads, snap_cap);
        for op in ops {
            match op {
                ConcOp::Snap { parent, child, .. } => fs.snaps.set_child(*parent, *child),
                ConcOp::Put { inode, offset, snap, value, seq } => {
                    fs.map.put_with_seq(0, &encode(*inode, *offset, *snap), value.clone(), *seq);
                }
            }
        }
        // keep a coherent seq_gen in case the recovered core is used further
        let maxseq = ops.iter().map(|o| o.seq()).max().unwrap_or(0);
        fs.seq_gen.store(maxseq + 1, Ordering::Relaxed);
        let _ = max_threads;
        fs
    }

    pub fn max_threads(&self) -> usize {
        self.max_threads
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_visibility() {
        let fs = ConcFs::new(8, 2, 1024);
        let root = fs.root_snapshot();
        fs.put(0, 1, 0, root, Value::Extent(100, 4));
        let child = fs.create_snapshot(0, root);
        assert_eq!(fs.get(1, 0, child), Some(Value::Extent(100, 4))); // inherited
        fs.put(0, 1, 0, child, Value::Extent(200, 8));
        assert_eq!(fs.get(1, 0, child), Some(Value::Extent(200, 8)));
        assert_eq!(fs.get(1, 0, root), Some(Value::Extent(100, 4))); // isolation
        fs.delete(0, 1, 0, child);
        assert_eq!(fs.get(1, 0, child), None);
        assert_eq!(fs.get(1, 0, root), Some(Value::Extent(100, 4)));
    }

    #[test]
    fn range_snapshot_consistent() {
        let fs = ConcFs::new(16, 2, 1024);
        let root = fs.root_snapshot();
        for off in [5u64, 1, 9, 3, 7] {
            fs.put(0, 1, off, root, Value::Extent(off, 1));
        }
        let child = fs.create_snapshot(0, root);
        fs.delete(0, 1, 5, child);
        fs.put(0, 1, 4, child, Value::Extent(444, 1));
        let at_root: Vec<u64> = fs.range(1, 0, 100, root).into_iter().map(|(o, _)| o).collect();
        assert_eq!(at_root, vec![1, 3, 5, 7, 9]);
        let at_child: Vec<u64> = fs.range(1, 0, 100, child).into_iter().map(|(o, _)| o).collect();
        assert_eq!(at_child, vec![1, 3, 4, 7, 9]);
    }

    #[test]
    fn recover_equals_live() {
        let fs = ConcFs::new(8, 2, 1024);
        let root = fs.root_snapshot();
        let c = fs.create_snapshot(0, root);
        for i in 0..100u64 {
            fs.put(0, i % 5, i, if i % 2 == 0 { root } else { c }, Value::Inode(i));
        }
        let ops = fs.drained_ops();
        let rec = ConcFs::recover(&ops, 8, 2, 1024);
        for i in 0..100u64 {
            for snap in [root, c] {
                assert_eq!(fs.get(i % 5, i, snap), rec.get(i % 5, i, snap));
            }
        }
    }
}
