# radix-fs-prototype

A userspace prototype **+ simulator** for the non-B+tree filesystem-core design
synthesized in [`../non-bplus-fs-core-exploration.md`](../non-bplus-fs-core-exploration.md):

> an **ordered, DRAM-authoritative adaptive radix trie**, journalled for
> durability, with **in-key snapshot ids** and a **wait-free write** path.

It builds the slice that exploration said carries the least risk to prototype
first — the parts with **no kernel / no real-crash-consistency burden**:
the ordered radix structure, snapshot visibility, journal replay (crash
recovery), range scans, and a **step-level model of the concurrency claims**.

`unsafe`-free (`#![forbid(unsafe_code)]`), **zero dependencies**, std-only.
Detached from the parent `portable-collections` workspace (own empty
`[workspace]`) so it does not touch the user-owned workspace `Cargo.toml`.

## Run

```sh
cargo test                                   # 14 tests: correctness, snapshots, crash recovery, concurrency
cargo run --release --bin simulate           # default 20k-op simulation report
cargo run --release --bin simulate -- 50000  # [steps] [seed]
```

## What it demonstrates (mapped to the four constraints)

| Constraint | How it shows up here | Evidence |
|---|---|---|
| **non-B+tree, ordered, ≥bcachefs structure** | byte-radix trie; keys `inode‖offset‖snap` big-endian ⇒ radix order = logical order | `key.rs`, `trie.rs`; range scans ordered |
| **soft-realtime (bounded reads)** | fixed-width keys ⇒ **constant depth**; lookups cost exactly `KEY_LEN` hops no matter how many keys | sim: *max exact-lookup hops = 20* at 27k keys |
| **no SMOs (the structural insight)** | insert only ever *adds* a node; nothing rotates/splits-propagates/merges | sim: ~2.05 nodes/key, grows with key bytes not rebalancing |
| **wait-free writes (beats bcachefs OLC)** | step-model writer completes in `KEY_LEN+1` of its own turns under any interleaving; single-step publish, **no retry** | sim: *random-interleave max writer turns = 21 = bound* |
| **the throughput tax you allowed** | `WaitFree` vs `LockFreeRetry` under N same-key spoilers | sim contrast: wait-free flat at 21; lock-free-retry 24→150 (unbounded tail) |
| **crash-consistency + snapshots** | append-only journal; recovery = replay a (possibly torn) prefix; in-key snapshot ancestry | sim: 0 crash mismatches; `snapshot_visibility` test |
| **correctness** | every read/range cross-checked vs an independent `BTreeMap` oracle | sim: 0 read/range mismatches over 50k ops |

## Design notes & honest stubs

- **Concurrency is a *model*, not real threads.** `conc.rs` advances each
  operation one bounded "step" per scheduler turn and an adversary interleaves
  them. This validates *wait-free = bounded steps under adversarial interleaving*
  and *linearizability* **without** unsafe atomics — the honest way to check the
  claims in safe single-threaded Rust. Real lock-free code (atomics, the
  fast-path/slow-path help mechanism, **Crystalline** reclamation) is the next
  step and is explicitly out of scope here.
- **Wait-free write, concretely.** Because radix has no structural rebalancing,
  a write is: descend the fixed-length path (bounded) then publish the leaf in a
  single step (the linearization point). There is no CAS-retry loop, so an
  adversary cannot starve a writer — the property `LockFreeRetry` lacks.
- **The trade you authorized.** Wait-free pays a fixed per-op cost always (it
  cannot take an optimistic shortcut), so it is slightly slower uncontended; in
  return its worst case is *bounded*. The contention table quantifies exactly
  this.
- **Adaptive nodes.** Children are a sorted `Vec<(u8, child)>` ("small node"). A
  production ART promotes hot nodes to a dense `[_;256]` array; that is a
  constant-factor optimization that changes no property under test.
- **Durability decoupling.** The authoritative index is the in-DRAM trie;
  durability is the separate journal (replayed on recovery). This is the move
  that dissolves the *wait-free-reads vs durable-linearizability* collision
  PACTree hit by putting the index in PMEM.

## Map to `portable-collections`

This is the userspace-first validation the exploration recommended, and it lines
up with the workspace's own `RadixBimap` / ART-backed roadmap (dense small-integer
keys → radix). The ordered-radix core, snapshot-consistent scans, and bounded-step
write model here are directly reusable as the basis of a safe, `no_std`,
`unsafe`-free radix collection — with the real lock-free + reclamation layer added
later, behind a feature, exactly as the bimap line deferred raw-pointer ART.

## Files

```
src/key.rs        fixed-width binary-comparable key encoding (constant trie depth)
src/snapshot.rs   snapshot ancestry + visibility (in-key snapshot model)
src/trie.rs       ordered byte-radix trie: insert / get / bounded-step lookup / range
src/store.rs      FsCore: snapshot-aware store + journal + crash recovery (replay)
src/journal.rs    write-ahead journal (durability authority); torn-prefix model
src/conc.rs       step-level concurrency model: wait-free writers, contention contrast
src/sim.rs        simulator: workload + differential oracle + crash test + concurrency
src/bin/simulate.rs   prints the report
tests/correctness.rs  end-to-end tests
```
