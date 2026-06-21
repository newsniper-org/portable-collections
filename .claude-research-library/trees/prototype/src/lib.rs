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
//!   `Arc`-refcount reclamation, O(shards) snapshots — `unsafe`-free.
//! * [`waitfree`] adds a **wait-free write** path (per-shard flat combining:
//!   gated fast path + announce/help + seq-stamped monotone apply) — the
//!   synthesized, adversarially-verified protocol. Per-op work is bounded
//!   `O(K + P)` regardless of contention; the hot-key stress shows max
//!   help-rounds/op stays a small constant while the lock-free retry tail grows.
//!
//! Remaining formal step (documented): a machine-checked (loom) model of the
//! gate+combine core; the current wait-free claim rests on the bound argument +
//! empirical bounded-rounds instrumentation.
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
/// Real, multi-threaded, **wait-free-write** CoW radix map (flat combining).
pub mod waitfree;

pub use key::{decode, encode, Inode, Offset, SnapId, KEY_LEN};
pub use snapshot::SnapshotTree;
pub use store::{FsCore, Value};
pub use trie::RadixTrie;
