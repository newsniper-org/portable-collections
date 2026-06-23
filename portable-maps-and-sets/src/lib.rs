#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

use portable_collection_primitives::{ifstd, ifstdoralloc};

ifstd!({
    #[allow(unused_imports)]
    use std::fmt;
} else {
    use portable_collection_primitives::ifalloc;
    ifalloc!({
        #[allow(unused_imports)]
        use core::fmt;
    });
});

// The radix maps need a heap; bring the `alloc` crate into scope on every tier
// that has one (alloc-only or std), so the `use alloc::...` paths in the radix
// submodules resolve regardless of whether `std` is also linked.
ifstdoralloc!({
    extern crate alloc;

    /// Ordered copy-on-write **radix maps** with O(1) snapshots.
    ///
    /// A `no_std`, `unsafe`-free byte-keyed map family: a persistent
    /// [`RadixOrderedMap`](radix::RadixOrderedMap), a path-compressed
    /// adaptive-radix [`ArtOrderedMap`](radix::ArtOrderedMap), and (with
    /// `--features concurrent`) their lock-free variants. See the module docs
    /// for the design rationale.
    pub mod radix;

    pub use radix::{ArtOrderedMap, OrderedMap, RadixOrderedMap, SnapshotMap};
});

// The concurrent variants live behind the `concurrent` feature (which implies
// `std`), so `radix` always exists when these re-exports are compiled.
#[cfg(feature = "concurrent")]
pub use radix::{ShardedArtOrderedMap, ShardedArtSnapshot, ShardedRadixOrderedMap, ShardedRadixSnapshot};
