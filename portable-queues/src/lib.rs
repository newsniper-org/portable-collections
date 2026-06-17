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

pub mod vec_scoped_stack;
// `VecScopedStack` needs `Vec` (alloc), so re-export it only on the alloc/std tiers.
ifstdoralloc!({
    pub use vec_scoped_stack::VecScopedStack;
});

pub mod deque_scoped_queue;
// `DequeScopedQueue` needs `VecDeque` (alloc), so re-export it only on the alloc/std tiers.
ifstdoralloc!({
    pub use deque_scoped_queue::DequeScopedQueue;
});

pub mod array_scoped_stack;
// `ArrayScopedStack` is alloc-free (fixed inline array), so it is available everywhere.
pub use array_scoped_stack::ArrayScopedStack;
