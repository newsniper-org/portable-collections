#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

use portable_collection_primitives::{ifstd, ifalloc};

ifstd!({
    #[allow(unused_imports)]
    use std::fmt;
} else {
    ifalloc!({
        extern crate alloc;
        #[allow(unused_imports)]
        use core::fmt;
    });
});
