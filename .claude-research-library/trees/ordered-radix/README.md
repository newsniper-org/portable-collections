# ordered-radix

A **`no_std`, `unsafe`-free ordered copy-on-write radix map** with **O(1)
snapshots**, plus an optional **lock-free concurrent** variant.

Extracted from the `portable-collections` lock-free FS-core prototype at the
request of `filesystem-researches` (see
`../.local-responses-from/filesystem-researches/2026-06-21-reply-...md`), for two
uses they named:

1. **on-disk backbone candidate** — the persistent `CowRadixMap` (default,
   `no_std`, **zero dependencies**). Immutable nodes shared via `alloc::sync::Arc`;
   every update path-copies the touched path; a snapshot is one `Arc` clone.
   Ordered (lexicographic = key order → range/predecessor), constant depth for
   fixed-width keys → bounded lookups, **no rebalancing / no SMOs** → simple to
   reason about and to verify, and CoW is the filesystem crash-consistency
   primitive (atomic root swing + free Merkle-able structure sharing).
2. **in-DRAM metadata cache** — `--features concurrent` adds
   `concurrent::ConcurrentRadixMap`: the same CoW structure with a lock-free
   atomic-`Arc` root (`arc-swap`): **wait-free reads, lock-free writes,
   `Arc`-refcount reclamation**, still `unsafe`-free. Keys shard by a
   configurable prefix (shard by the inode prefix → per-inode reads/scans are
   single-shard/local; different inodes spread for write concurrency).

Keys are byte slices (the FS uses fixed-width `inode‖h64‖cd`); values are generic
`V: Clone` (CoW path-copy clones nodes).

## API

```rust
use ordered_radix::{CowRadixMap, OrderedMap, SnapshotMap};

let mut m: CowRadixMap<u64> = CowRadixMap::new();
m.insert(b"\x00\x00\x00\x2a/0001", 100);     // insert
let _ = m.get(b"\x00\x00\x00\x2a/0001");      // lookup
let _ = m.range(b"\x00\x00\x00\x2a/", b"\x00\x00\x00\x2a/\xff"); // range
let snap = m.snapshot();                      // O(1), isolated
```

`OrderedMap` = `insert / get / range / len`; `SnapshotMap` = `snapshot`.

## Build / test

```sh
cargo build                      # no_std, zero-dependency persistent core
cargo test                       # persistent-core tests
cargo test --features concurrent # + real-threads lock-free tests
```

`cargo build` (no features) proves the library is `no_std` (the crate is
`#![no_std]` except under `cfg(test)` and the `concurrent` feature, which need
`std` for the test harness / `arc-swap`).

## LVIAARC backbone interface (concurrent ART)

`ConcurrentArt` (the `--features concurrent` lock-free, **seq-stamped** ART) is
the backbone for `filesystem-researches`' LVIAARC write-back cache. Beyond
`insert`/`get`/`snapshot` it exposes the thin cache-facing contract:

- `apply(key, value, op_seq)` — monotone apply (a write lands only if `op_seq`
  exceeds the resident one; the caller owns the op-sequence space — only per-key
  monotonicity is required).
- `apply_batch(&[(key, value, op_seq)])` — **fold a flush batch into one root
  transition per shard** (atomic, monotone, order-independent) — the LVIAARC
  flush primitive (public generalization of the prototype's wait-free `help`).
- `key_seq(key) -> Option<u64>` — per-key integrated generation (recovery
  dominance query); `integrated_generation() -> u64` — coarse max applied seq;
  `shard_max_seq(s)` + `shard_index(key)` + `num_shards()` — **per-shard** max
  seq so recovery bounds each shard's scan independently.
- node-type growth (N4→256) is a CoW **node replacement**, so batches commit
  SMO-free as a single root CAS — wait-free reads, lock-free writes preserved.

## Notes & scope

- The **persistent core is zero-dep**; only the `concurrent` feature pulls
  `arc-swap` (one vetted crate; keeps the lock-free code `unsafe`-free).
- A genuinely **wait-free *write*** path (per-shard flat combining, loom-checked)
  lives in the source prototype
  (`../prototype/src/waitfree.rs`); this crate ships the lock-free write baseline,
  which suits the in-DRAM cache use. Promote later if wait-free writes are needed.
- The full FS stack (in-key snapshots + journal durability) is in
  `../prototype/src/concurrent.rs`; this crate is just the reusable map layer.
