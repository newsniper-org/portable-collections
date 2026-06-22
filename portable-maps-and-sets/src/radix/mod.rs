//! Ordered copy-on-write **radix maps** with **O(1) snapshots** — a `no_std`,
//! `unsafe`-free byte-keyed map family, plus optional **lock-free concurrent**
//! variants (`--features concurrent`).
//!
//! Ported into `portable-collections` from the `ordered-radix` research crate
//! (extracted for `filesystem-researches`). Two backbone uses motivated it:
//!
//!   1. **persistent / on-disk backbone** — [`CowRadixMap`] (a naive byte-radix)
//!      and [`ArtCowMap`] (a path-compressed **Adaptive Radix Tree**). Both are
//!      `no_std` + zero-runtime-dependency: immutable nodes shared via
//!      `alloc::sync::Arc`, every update path-copies the touched path, and a
//!      snapshot is a single `Arc` clone — O(1) and fully isolated from later
//!      writes (the persistent-structure property, and the CoW crash-consistency
//!      primitive a filesystem wants). Ordered (lexicographic = key order) →
//!      range/predecessor scans; no rebalancing / no structure-modifying ops
//!      (SMOs) → simple to reason about and to verify.
//!   2. **in-DRAM metadata cache** — with `--features concurrent`,
//!      [`ConcurrentRadixMap`] and the seq-stamped [`ConcurrentArt`] give the
//!      same CoW structure a lock-free atomic-`Arc` root (`arc-swap`):
//!      **wait-free reads, lock-free writes, `Arc`-refcount reclamation**, still
//!      `unsafe`-free. Keys shard by a configurable prefix so per-prefix scans
//!      stay single-shard.
//!
//! Keys are byte slices (a filesystem uses fixed-width `inode‖h64‖cd`); values
//! are generic `V: Clone` (CoW path-copy clones nodes, hence `Clone`).
//!
//! [`CowRadixMap`]: crate::radix::CowRadixMap
//! [`ArtCowMap`]: crate::radix::ArtCowMap
//! [`ConcurrentRadixMap`]: crate::radix::ConcurrentRadixMap
//! [`ConcurrentArt`]: crate::radix::ConcurrentArt

mod art;
mod cow;
mod traits;

pub use art::ArtCowMap;
pub use cow::CowRadixMap;
pub use traits::{OrderedMap, SnapshotMap};

#[cfg(feature = "concurrent")]
mod concurrent;

#[cfg(feature = "concurrent")]
pub use art::{ArtSnapshot, ConcurrentArt};
#[cfg(feature = "concurrent")]
pub use concurrent::ConcurrentRadixMap;
