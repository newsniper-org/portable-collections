#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

use portable_collection_primitives::{ifstd, ifstdoralloc};

ifstd!({
    #[allow(unused_imports)]
    use std::fmt;
} else {
    use portable_collection_primitives::{ifalloc};
    ifalloc!({
        extern crate alloc;
        #[allow(unused_imports)]
        use core::fmt;
    });
});

pub mod vec_log;
// `VecLog` needs `Vec` (alloc), so re-export it only on the alloc/std tiers.
ifstdoralloc!({
    pub use vec_log::VecLog;
});

pub mod vec_queue;
// `VecQueue` needs `VecDeque` (alloc), so re-export it only on the alloc/std tiers.
ifstdoralloc!({
    pub use vec_queue::VecQueue;
});

pub mod heapless_log;
// `HeaplessLog` is alloc-free (fixed inline array), so it is available everywhere.
pub use heapless_log::HeaplessLog;
