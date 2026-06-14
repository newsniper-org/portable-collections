#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

/// Split a chunk of items into a `std` branch and a non-`std` branch.
///
/// Each item in the `std` branch is annotated with
/// `#[cfg(feature = "std")]`; each item in the `else` branch is annotated
/// with `#[cfg(not(feature = "std"))]`. The `else` branch is optional.
///
/// The `feature = "std"` predicate is evaluated in the *invoking* crate's
/// context, so any crate using `ifstd!` must declare its own `std` feature.
///
/// # Invocation syntax
///
/// Rust's macro grammar doesn't allow a free-standing `else` after a
/// `name!{...}` invocation (the closing `}` ends the macro call), so the
/// invocation itself is paren-delimited and the two branches live inside:
///
/// ```text
/// ifstd!({
///     /* std items */
/// } else {
///     /* non-std items */
/// });
/// ```
///
/// # Limitations
///
/// The macro uses the `:item` matcher, so each branch must contain a
/// sequence of complete Rust items (`use`, `fn`, `struct`, `impl`, …).
/// Statement-level or expression-level code is not accepted — for those
/// cases use `#[cfg(feature = "std")]` directly.
///
/// # Examples
///
/// Two-branch form (one of the two functions is emitted depending on
/// whether the calling crate has the `std` feature on):
///
/// ```
/// # use portable_collection_primitives::ifstd;
/// ifstd!({
///     pub fn platform_name() -> &'static str { "std" }
/// } else {
///     pub fn platform_name() -> &'static str { "no_std" }
/// });
/// # assert!(matches!(platform_name(), "std" | "no_std"));
/// ```
///
/// Single-branch form (no `else`) when only `std`-only items are needed.
/// `pub fn enabled` is present iff the invoking crate has `std`; in this
/// doc-test the test crate has no `std` feature so `enabled` is absent
/// and the function is never referenced:
///
/// ```
/// # use portable_collection_primitives::ifstd;
/// ifstd!({
///     pub fn enabled() -> bool { true }
/// });
/// ```
#[macro_export]
macro_rules! ifstd {
    (
        { $($if_std:item)* }
        $( else { $($if_not_std:item)* } )?
    ) => {
        $( #[cfg(feature = "std")] $if_std )*
        $( $( #[cfg(not(feature = "std"))] $if_not_std )* )?
    };
}

/// Same as [`ifstd!`] but keyed on the `alloc` feature instead of `std`.
///
/// Useful when an item should be available in alloc-only builds but not
/// in the bare-`no_std` (no heap) build.
///
/// # Examples
///
/// ```
/// # use portable_collection_primitives::ifalloc;
/// ifalloc!({
///     pub fn tag() -> &'static str { "alloc" }
/// } else {
///     pub fn tag() -> &'static str { "no_alloc" }
/// });
/// # assert!(matches!(tag(), "alloc" | "no_alloc"));
/// ```
#[macro_export]
macro_rules! ifalloc {
    (
        { $($if_alloc:item)* }
        $( else { $($if_not_alloc:item)* } )?
    ) => {
        $( #[cfg(feature = "alloc")] $if_alloc )*
        $( $( #[cfg(not(feature = "alloc"))] $if_not_alloc )* )?
    };
}

/// Same as [`ifstd!`] but keyed on at least one of `alloc` and the `std` features.
///
/// Useful when an item should be available in alloc-only builds but not
/// in the bare-`no_std` (no heap) build.
///
/// # Examples
///
/// ```
/// # use portable_collection_primitives::ifstdoralloc;
/// ifstdoralloc!({
///     pub fn tag() -> &'static str { "std_or_alloc" }
/// } else {
///     pub fn tag() -> &'static str { "no_std_no_alloc" }
/// });
/// # assert!(matches!(tag(), "std_or_alloc" | "no_std_no_alloc"));
/// ```
#[macro_export]
macro_rules! ifstdoralloc {
    (
        { $($if_std_or_alloc:item)* }
        $( else { $($if_none:item)* } )?
    ) => {
        $( #[cfg(any(feature = "std", feature = "alloc"))] $if_std_or_alloc )*
        $( $( #[cfg(not(any(feature = "std", feature = "alloc")))] $if_none )* )?
    };
}


/// Fan an attribute prefix out across a sequence of items.
///
/// The first brace block lists attributes to apply; the second brace
/// block lists one or more items. Each item in the body is emitted with
/// every attribute prefix attached, in source order.
///
/// This is convenient for things like stamping the same `#[cfg(...)]`,
/// `#[allow(...)]`, or `#[inline]` onto a small batch of definitions
/// without copy-pasting the attribute line on every item.
///
/// # Invocation syntax
///
/// Both blocks are mandatory:
///
/// ```text
/// group! {
///     { #[attr1] #[attr2] ... }
///     { item1 item2 ... }
/// }
/// ```
///
/// # Limitations
///
/// The body uses the `:item` matcher, so each entry must be a complete
/// Rust item (`use`, `fn`, `struct`, `impl`, …). Statement- or
/// expression-level code is not accepted. The attribute block accepts
/// the full attribute grammar via `tt`, but malformed input surfaces as
/// macro-expansion errors rather than friendly diagnostics.
///
/// # Examples
///
/// Stamp `#[allow(dead_code)]` and `#[inline]` onto a pair of items:
///
/// ```
/// # use portable_collection_primitives::group;
/// group! {
///     { #[allow(dead_code)] #[inline] }
///     {
///         pub fn alpha() -> i32 { 1 }
///         pub fn beta() -> i32 { 2 }
///     }
/// }
/// # assert_eq!(alpha() + beta(), 3);
/// ```
#[macro_export]
macro_rules! group {
    // Entry point: { attrs } { items }
    (
        { $($attrs:tt)* }
        { $($body:tt)* }
    ) => {
        $crate::group!(@each [$($attrs)*] $($body)*);
    };

    // Base case: no items left.
    (@each [$($attrs:tt)*]) => {};

    // Recurse: peel one item off the front and re-attach the attribute prefix.
    (@each [$($attrs:tt)*] $item:item $($rest:tt)*) => {
        $($attrs)* $item
        $crate::group!(@each [$($attrs)*] $($rest)*);
    };
}

/// Stamp out the same set of trait impls for each of several receiver types.
///
/// The first brace block is a comma-separated list of receiver types;
/// the second is a sequence of `impl Trait { ... }` blocks. Every impl
/// block is emitted once per type, so `N` types × `M` impl blocks
/// produces `N * M` impls total. Attribute prefixes
/// (`#[cfg(...)]`, `#[allow(...)]`, …) and `where` clauses on the impl
/// headers are preserved per emission.
///
/// # Invocation syntax
///
/// ```text
/// implgroup_for! {
///     { Type1, Type2, ... }
///     {
///         #[some_attr]
///         impl TraitA { fn a(&self) { /* ... */ } }
///         impl TraitB where Self: Sized { fn b(&self) { /* ... */ } }
///         ...
///     }
/// }
/// ```
///
/// # Limitations
///
/// The body is parsed as raw `tt` and torn apart by an internal
/// state machine, so:
///
/// - Each item must start with zero or more `#[...]` attributes and
///   then literal `impl`. Other top-level item kinds are not accepted.
/// - Generic parameter lists on the `impl` header are supported (they
///   fall through the token-accumulation arm), but `where` clauses must
///   come *between* the trait spec and the `{ ... }` body, as in
///   ordinary Rust.
/// - Malformed input typically surfaces as cryptic
///   `expected one of ...` errors at the expansion site.
///
/// # Examples
///
/// One trait, two primitive receivers:
///
/// ```
/// # use portable_collection_primitives::implgroup_for;
/// trait Tag { fn tag(&self) -> &'static str; }
/// implgroup_for! {
///     { u8, i32 }
///     {
///         impl Tag { fn tag(&self) -> &'static str { "number" } }
///     }
/// }
/// # assert_eq!(1u8.tag(), "number");
/// # assert_eq!(1i32.tag(), "number");
/// ```
#[macro_export]
macro_rules! implgroup_for {
    //—————————————————————————————————
    // Entry point. Peels one type off the front and recurses on the
    // rest, so that the body's `tt` group can be re-expanded per type
    // without running into a `repeats N vs M times` metavar mismatch
    // between the type list and the body tokens.
    //—————————————————————————————————
    ( { $(,)? } { $($body:tt)* } ) => {};
    (
        { $ty:ty $(, $rest:ty)* $(,)? }
        { $($body:tt)* }
    ) => {
        $crate::implgroup_for!(@for_type $ty; $($body)*);
        $crate::implgroup_for! { { $($rest),* } { $($body)* } }
    };

    //—————————————————————————————————
    // Iterate the body once per type.
    //—————————————————————————————————

    (@for_type $ty:ty;) => {};

    // Attribute spotted: start collecting attributes for this impl.
    (@for_type $ty:ty; #[$($attr:tt)*] $($rest:tt)*) => {
        implgroup_for!(@attrs [$ty] [#[$($attr)*]] $($rest)*);
    };

    // No attributes — straight into the impl header.
    (@for_type $ty:ty; impl $($rest:tt)*) => {
        implgroup_for!(@trait [$ty] [] [] $($rest)*);
    };

    //—————————————————————————————————
    // @attrs: accumulate a run of `#[...]` attributes.
    //—————————————————————————————————

    // Another attribute — keep accumulating.
    (@attrs [$ty:ty] [$($attrs:tt)*] #[$($attr:tt)*] $($rest:tt)*) => {
        implgroup_for!(@attrs [$ty] [$($attrs)* #[$($attr)*]] $($rest)*);
    };

    // `impl` reached — switch to trait-spec accumulation.
    (@attrs [$ty:ty] [$($attrs:tt)*] impl $($rest:tt)*) => {
        implgroup_for!(@trait [$ty] [$($attrs)*] [] $($rest)*);
    };

    //—————————————————————————————————
    // @trait: accumulate the trait spec (everything between `impl` and
    // the body or `where`). `[$attrs]` carries the collected attributes
    // through.
    //—————————————————————————————————

    // `{ body }` reached — emit the impl.
    (@trait [$ty:ty] [$($attrs:tt)*] [$($spec:tt)*] { $($body:tt)* } $($next:tt)*) => {
        $($attrs)* impl $($spec)* for $ty { $($body)* }
        implgroup_for!(@for_type $ty; $($next)*);
    };

    // `where` reached — switch to where-clause accumulation.
    (@trait [$ty:ty] [$($attrs:tt)*] [$($spec:tt)*] where $($rest:tt)*) => {
        implgroup_for!(@where [$ty] [$($attrs)*] [$($spec)*] [] $($rest)*);
    };

    // Accumulate one more token of the trait spec.
    (@trait [$ty:ty] [$($attrs:tt)*] [$($spec:tt)*] $tok:tt $($rest:tt)*) => {
        implgroup_for!(@trait [$ty] [$($attrs)*] [$($spec)* $tok] $($rest)*);
    };

    //—————————————————————————————————
    // @where: accumulate the `where` clause until the body opens.
    //—————————————————————————————————

    // `{ body }` reached — emit the impl with the where clause.
    (@where [$ty:ty] [$($attrs:tt)*] [$($spec:tt)*] [$($wh:tt)*] { $($body:tt)* } $($next:tt)*) => {
        $($attrs)* impl $($spec)* for $ty where $($wh)* { $($body)* }
        implgroup_for!(@for_type $ty; $($next)*);
    };

    // Accumulate one more where-clause token.
    (@where [$ty:ty] [$($attrs:tt)*] [$($spec:tt)*] [$($wh:tt)*] $tok:tt $($rest:tt)*) => {
        implgroup_for!(@where [$ty] [$($attrs)*] [$($spec)*] [$($wh)* $tok] $($rest)*);
    };
}


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



pub mod primitives;
pub use primitives::{Checkpoint, ScopedRollback};
