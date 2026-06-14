#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

use portable_collection_primitives::{ifstd};

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
