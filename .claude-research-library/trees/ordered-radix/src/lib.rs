//! `ordered-radix` — a `no_std`, `unsafe`-free **ordered copy-on-write radix
//! map** with **O(1) snapshots**, plus an optional **lock-free concurrent**
//! variant.
//!
//! Extracted from the `portable-collections` lock-free FS-core prototype at the
//! request of `filesystem-researches`, for two uses:
//!   1. **on-disk backbone candidate** — the persistent [`CowRadixMap`] is
//!      `no_std` + zero-dependency + `forbid(unsafe)`: an immutable-node radix
//!      tree where every update path-copies and a snapshot is a single `Arc`
//!      clone. Ordered (lexicographic = key order) → range/predecessor scans;
//!      constant depth for fixed-width keys → bounded lookups; no rebalancing
//!      (no SMOs) → simple to reason about and (later) to verify.
//!   2. **in-DRAM metadata cache** — enable `--features concurrent` for
//!      [`concurrent::ConcurrentRadixMap`]: the same CoW structure with a
//!      lock-free atomic-`Arc` root (wait-free reads, lock-free writes,
//!      `Arc`-refcount reclamation), still `unsafe`-free (via the vetted
//!      `arc-swap`).
//!
//! Keys are byte slices (the FS uses fixed-width `inode‖h64‖cd`); values are
//! generic `V: Clone` (CoW path-copy clones nodes, hence `Clone`).
// `no_std` for real builds; `std` under `cargo test` (test harness) and when the
// `concurrent` feature is on (arc-swap). `cargo build` with no features proves
// the library itself is `no_std`.
#![cfg_attr(all(not(feature = "concurrent"), not(test)), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod art;
mod cow;
pub mod traits;

pub use art::ArtCowMap;
pub use cow::CowRadixMap;
pub use traits::{OrderedMap, SnapshotMap};

#[cfg(feature = "concurrent")]
pub use art::ConcurrentArt;

#[cfg(feature = "concurrent")]
pub mod concurrent;
