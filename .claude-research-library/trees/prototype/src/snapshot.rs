//! Snapshot ancestry (the "snapshots btree" of the design, simplified).
//!
//! Snapshots form a tree (parent -> child). A version written at snapshot `W`
//! is **visible** to a read at snapshot `R` iff `W` is an ancestor-or-equal of
//! `R`. Among all visible versions of a key, the one written at the *nearest*
//! ancestor (greatest depth) wins. This is bcachefs's in-key snapshot model:
//! O(1) snapshot creation, snapshot-consistent (not live-linearizable) reads.

use crate::key::SnapId;

#[derive(Clone, Debug)]
pub struct SnapshotTree {
    // Indexed by id. Index 0 is a sentinel ("no parent"); the root is id 1.
    parent: Vec<SnapId>,
    depth: Vec<u32>,
}

impl Default for SnapshotTree {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotTree {
    /// A fresh tree with a single root snapshot (id 1, depth 0).
    pub fn new() -> Self {
        Self {
            parent: vec![0, 0], // [sentinel, root]
            depth: vec![0, 0],
        }
    }

    #[inline]
    pub fn root(&self) -> SnapId {
        1
    }

    /// Create a child snapshot of `parent`; returns the new id. O(1).
    pub fn add_child(&mut self, parent: SnapId) -> SnapId {
        assert!((parent as usize) < self.parent.len() && parent != 0, "bad parent snapshot");
        let id = self.parent.len() as SnapId;
        self.parent.push(parent);
        self.depth.push(self.depth[parent as usize] + 1);
        id
    }

    #[inline]
    pub fn exists(&self, id: SnapId) -> bool {
        id != 0 && (id as usize) < self.parent.len()
    }

    #[inline]
    pub fn depth(&self, id: SnapId) -> u32 {
        self.depth[id as usize]
    }

    /// Number of snapshots (excluding the sentinel).
    #[inline]
    pub fn count(&self) -> SnapId {
        self.parent.len() as SnapId - 1
    }

    /// Is `anc` an ancestor-or-equal of `node`?
    pub fn is_ancestor_or_eq(&self, anc: SnapId, mut node: SnapId) -> bool {
        loop {
            if node == anc {
                return true;
            }
            if node == 0 {
                return false;
            }
            node = self.parent[node as usize];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ancestry() {
        let mut t = SnapshotTree::new();
        let root = t.root();
        let a = t.add_child(root);
        let b = t.add_child(a);
        let c = t.add_child(root); // sibling subtree

        assert!(t.is_ancestor_or_eq(root, b));
        assert!(t.is_ancestor_or_eq(a, b));
        assert!(t.is_ancestor_or_eq(b, b));
        assert!(!t.is_ancestor_or_eq(b, a)); // child is not ancestor of parent
        assert!(!t.is_ancestor_or_eq(c, b)); // siblings unrelated
        assert_eq!(t.depth(root), 0);
        assert_eq!(t.depth(b), 2);
        assert_eq!(t.count(), 4);
    }
}
