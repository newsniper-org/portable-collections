//! Write-ahead journal (durability authority).
//!
//! In the design, durability is a lock-free append-only log; the DRAM trie is
//! reconstructed by replaying it on mount. Here it is an in-memory `Vec<Op>`; a
//! "crash" is modeled by replaying a *prefix* of the log (a torn tail), and the
//! recovered state must match applying exactly that prefix — the durable-prefix
//! property.

use crate::key::{Inode, Offset, SnapId};
use crate::store::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Put {
        inode: Inode,
        offset: Offset,
        snap: SnapId,
        value: Value,
    },
    Snap {
        parent: SnapId,
    },
}

#[derive(Clone, Debug, Default)]
pub struct Journal {
    ops: Vec<Op>,
}

impl Journal {
    pub fn new() -> Self {
        Journal { ops: Vec::new() }
    }

    pub fn from_ops(ops: Vec<Op>) -> Self {
        Journal { ops }
    }

    #[inline]
    pub fn append(&mut self, op: Op) {
        self.ops.push(op);
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    #[inline]
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    /// The durable prefix of length `n` (simulates a crash that lost the tail).
    pub fn prefix(&self, n: usize) -> Vec<Op> {
        self.ops[..n.min(self.ops.len())].to_vec()
    }
}
