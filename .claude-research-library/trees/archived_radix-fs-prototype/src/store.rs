//! `FsCore`: the snapshot-aware ordered store built on the radix trie, plus a
//! journal for durability. This is the DRAM-authoritative index of the design;
//! durability is the *separate* journal (which dissolves the
//! wait-free-read vs durable-linearizability collision PACTree hit by putting
//! the index in PMEM).

use crate::journal::{Journal, Op};
use crate::key::{decode, encode, Inode, Offset, SnapId};
use crate::snapshot::SnapshotTree;
use crate::trie::RadixTrie;

/// A filesystem value. `Tombstone` is a whiteout (snapshot-scoped delete).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// Extent: (physical block, length in blocks).
    Extent(u64, u32),
    /// Directory entry: child inode number.
    Dirent(u64),
    /// Inode metadata blob (opaque here).
    Inode(u64),
    /// Whiteout: the key is deleted as of this snapshot.
    Tombstone,
}

/// Resolve the visible version of a key for a read at `read` snapshot.
/// `candidates` is the set of `(written_snapshot, value)` versions of one key.
/// Returns `None` if no visible version, or the visible version is a tombstone.
pub fn resolve(candidates: &[(SnapId, Value)], read: SnapId, snaps: &SnapshotTree) -> Option<Value> {
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

pub struct FsCore {
    trie: RadixTrie<Value>,
    pub snaps: SnapshotTree,
    pub journal: Journal,
}

impl Default for FsCore {
    fn default() -> Self {
        Self::new()
    }
}

impl FsCore {
    pub fn new() -> Self {
        FsCore {
            trie: RadixTrie::new(),
            snaps: SnapshotTree::new(),
            journal: Journal::new(),
        }
    }

    /// Apply a mutation to the in-memory state *without* journaling. Used both by
    /// the journaling ops below and by `replay` during crash recovery.
    pub fn apply(&mut self, op: &Op) {
        match *op {
            Op::Put { inode, offset, snap, ref value } => {
                self.trie.insert(&encode(inode, offset, snap), value.clone());
            }
            Op::Snap { parent } => {
                self.snaps.add_child(parent);
            }
        }
    }

    /// Write a value at `(inode, offset)` in snapshot `snap`. Journals first.
    pub fn put(&mut self, inode: Inode, offset: Offset, snap: SnapId, value: Value) {
        let op = Op::Put { inode, offset, snap, value };
        self.journal.append(op.clone());
        self.apply(&op);
    }

    /// Snapshot-scoped delete (writes a tombstone).
    pub fn delete(&mut self, inode: Inode, offset: Offset, snap: SnapId) {
        self.put(inode, offset, snap, Value::Tombstone);
    }

    /// Create a child snapshot of `parent`; returns the new id.
    pub fn create_snapshot(&mut self, parent: SnapId) -> SnapId {
        self.journal.append(Op::Snap { parent });
        self.snaps.add_child(parent)
    }

    /// All `(snapshot, value)` versions of `(inode, offset)`, in snapshot order.
    fn versions(&self, inode: Inode, offset: Offset) -> Vec<(SnapId, Value)> {
        let lo = encode(inode, offset, 0);
        let hi = encode(inode, offset, u32::MAX);
        self.trie
            .range_inclusive(&lo, &hi)
            .into_iter()
            .map(|(k, v)| {
                let (_, _, snap) = decode(&k);
                (snap, v)
            })
            .collect()
    }

    /// Snapshot-consistent point read.
    pub fn get(&self, inode: Inode, offset: Offset, read: SnapId) -> Option<Value> {
        resolve(&self.versions(inode, offset), read, &self.snaps)
    }

    /// Snapshot-consistent ordered range scan over `offset` in `[lo_off, hi_off)`
    /// for `inode`, at `read` snapshot. Returns `(offset, value)` ascending.
    pub fn range(&self, inode: Inode, lo_off: Offset, hi_off: Offset, read: SnapId) -> Vec<(Offset, Value)> {
        let lo = encode(inode, lo_off, 0);
        let hi = encode(inode, hi_off, u32::MAX);
        let raw = self.trie.range_inclusive(&lo, &hi);

        let mut out = Vec::new();
        let mut i = 0;
        while i < raw.len() {
            let (_, cur_off, _) = decode(&raw[i].0);
            if cur_off >= hi_off {
                i += 1;
                continue;
            }
            // Gather all versions of this offset (contiguous in key order).
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

    /// Exact-key point lookup that reports node hops — demonstrates that a read
    /// costs a bounded `KEY_LEN` hops regardless of store size (soft-realtime).
    pub fn exact_lookup_steps(&self, inode: Inode, offset: Offset, snap: SnapId) -> (Option<Value>, u32) {
        let (v, steps) = self.trie.get_with_steps(&encode(inode, offset, snap));
        (v.cloned(), steps)
    }

    pub fn trie_nodes(&self) -> usize {
        self.trie.node_count()
    }

    pub fn trie_len(&self) -> usize {
        self.trie.len()
    }
}

/// Rebuild an `FsCore` by replaying a journal prefix — the crash-recovery path.
/// `ops` is the durable prefix; the recovered core must equal the live state
/// produced by the same prefix.
pub fn replay(ops: &[Op]) -> FsCore {
    let mut core = FsCore::new();
    for op in ops {
        core.apply(op);
    }
    core.journal = Journal::from_ops(ops.to_vec());
    core
}
