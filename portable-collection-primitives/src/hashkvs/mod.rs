
ifstd!({
    use std::collections::{HashMap, HashSet, hash_set::Union};
    use std::hash::{Hash, BuildHasher};

    #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
    use std::alloc::Allocator;

    #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
    wrap_into_map_traits!(HashMap<S: [BuildHasher], A: [Allocator]>, K: [Hash + Eq], Q: [Hash + Eq]);
    #[cfg(all(feature = "unstable", not(toolchain_channel = "stable")))]
    wrap_into_set_traits!(HashSet<S: [BuildHasher], A: [Allocator]>, Union<S, A>, T: [Hash + Eq], Q: [Hash + Eq]);
    #[cfg(not(all(feature = "unstable", not(toolchain_channel = "stable"))))]
    wrap_into_map_traits!(HashMap<S: [BuildHasher]>, K: [Hash + Eq], Q: [Hash + Eq]);
    #[cfg(not(all(feature = "unstable", not(toolchain_channel = "stable"))))]
    wrap_into_set_traits!(HashSet<S: [BuildHasher]>, Union<S>, T: [Hash + Eq], Q: [Hash + Eq]);
});