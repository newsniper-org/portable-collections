ifstd!({
    pub(crate) use std::collections::{BTreeMap, BTreeSet};
    use std::collections::btree_set::Union;

    #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
    use std::alloc::Allocator;
} else {
    ifalloc!({
        extern crate alloc;
        pub(crate) use alloc::collections::{BTreeMap, BTreeSet};
        use alloc::collections::btree_set::Union;
    });
    #[allow(unused_imports)]
    use core::cmp::Ord;

    #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))] 
    use core::alloc::Allocator;
});




ifstdoralloc!({
    #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
    wrap_into_map_traits!(BTreeMap<A: [Allocator + Clone]>, K: [Ord], Q: [Ord]);
    #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
    wrap_into_set_traits!(BTreeSet<A: [Allocator + Clone]>, Union, T: [Ord], Q: [Ord]);
    #[cfg(not(all(feature = "unstable", not(toolchain_channel = "stable"))))]
    wrap_into_map_traits!(BTreeMap, K: [Ord], Q: [Ord]);
    #[cfg(not(all(feature = "unstable", not(toolchain_channel = "stable"))))]
    wrap_into_set_traits!(BTreeSet, Union, T: [Ord], Q: [Ord]);
});