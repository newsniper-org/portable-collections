#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(all(feature = "unstable", not(toolchain_channel = "stable")), feature(allocator_api))]
#![cfg_attr(all(feature = "unstable", not(toolchain_channel = "stable")), feature(btreemap_alloc))]


use portable_collection_primitives::{ifstd, ifstdoralloc};

ifstd!({
} else {
    use portable_collection_primitives::ifalloc;
    ifalloc!({
        extern crate alloc;
    });
});

pub mod btree_bimap;
pub mod flat_radix_bimap;
pub use flat_radix_bimap::DenseIndex; // heap-free, available in every tier
// `BTreeBimap`/`FlatRadixBimap`/`InsertError` live behind `ifstdoralloc!`, so
// re-export them only where they exist (alloc or std); a bare-no_std re-export
// would be unresolved.
ifstdoralloc!({
    pub use btree_bimap::{BTreeBimap, InsertError};
    pub use flat_radix_bimap::FlatRadixBimap;
});