//! Userspace prototype of the **non-B+tree FS-core synthesis**:
//! an *ordered, DRAM-authoritative adaptive radix trie*, journalled for
//! durability, with *in-key snapshot ids* and a *wait-free write* model.
//!
//! This crate deliberately implements the slice the design exploration said
//! carries the least risk to prototype first (no kernel / no crash-consistency
//! burden beyond a simulated journal): the ordered radix structure, snapshot
//! visibility, journal replay / crash recovery, range scans, and a step-level
//! *model* of the concurrency claims (wait-free reads + wait-free bounded-step
//! writes), validated by a simulator with a differential oracle.
//!
//! Two concurrency layers:
//! * [`conc`] *models* concurrency at the step level to validate the wait-free
//!   write bound + linearizability without unsafe (single-threaded).
//! * [`lockfree`] is the **real, multi-threaded, lock-free** CoW radix map
//!   (atomics via `arc-swap`): wait-free reads, lock-free sharded writes,
//!   `Arc`-refcount reclamation, O(shards) snapshots — still `unsafe`-free.
//!
//! Still open (documented): a *formally wait-free* write path (descriptor +
//! helping, or ART-style mutable nodes with Crystalline reclamation) — writes in
//! [`lockfree`] are lock-free, not wait-free.
#![forbid(unsafe_code)]

pub mod key;
pub mod snapshot;
pub mod trie;
pub mod store;
pub mod journal;
pub mod conc;
pub mod sim;
/// Real, multi-threaded, lock-free CoW radix map (atomics via `arc-swap`).
pub mod lockfree;

pub use key::{decode, encode, Inode, Offset, SnapId, KEY_LEN};
pub use snapshot::SnapshotTree;
pub use store::{FsCore, Value};
pub use trie::RadixTrie;
