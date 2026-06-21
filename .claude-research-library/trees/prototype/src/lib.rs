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
//! Out of scope (documented stubs): real atomics / threads / Crystalline
//! reclamation. The concurrency properties are demonstrated as *model*
//! properties (bounded steps per op under adversarial interleaving +
//! linearizability), which is the honest way to validate them in safe,
//! `unsafe`-free, single-threaded Rust.
#![forbid(unsafe_code)]

pub mod key;
pub mod snapshot;
pub mod trie;
pub mod store;
pub mod journal;
pub mod conc;
pub mod sim;

pub use key::{decode, encode, Inode, Offset, SnapId, KEY_LEN};
pub use snapshot::SnapshotTree;
pub use store::{FsCore, Value};
pub use trie::RadixTrie;
