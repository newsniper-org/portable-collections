# radix-fs-prototype

A userspace prototype **+ simulator** for the non-B+tree filesystem-core design
synthesized in [`../non-bplus-fs-core-exploration.md`](../non-bplus-fs-core-exploration.md):

> an **ordered, DRAM-authoritative adaptive radix trie**, journalled for
> durability, with **in-key snapshot ids** and a **wait-free write** path.

It now assembles the **full stack** (`concurrent::ConcFs`): a real
multi-threaded **wait-free-read / wait-free-write** radix map + in-key snapshots
+ journal durability + snapshot-consistent range scans, validated under real
threads and crash recovery. It grew in layers, each verified before the next:
sequential core → concurrency *model* → real **lock-free** map → real
**wait-free** writes (loom-checked) → the composed stack.

`unsafe`-free (`#![forbid(unsafe_code)]`). The sequential core is dependency-free;
the **real lock-free layer** (`lockfree.rs`) uses one vetted crate, `arc-swap`,
for the atomic `Arc` swap — so even the lock-free code stays `unsafe`-free
(reclamation is `Arc` refcounting). Detached from the parent `portable-collections`
workspace (own empty `[workspace]`) so it does not touch the user-owned workspace
`Cargo.toml`.

## Run

```sh
cargo test                                   # 31 tests: sequential, crash recovery, lock-free, wait-free, full-stack
cargo run --release --bin simulate           # sequential-core + concurrency-model report
cargo run --release --bin stress             # REAL multi-threaded lock-free + wait-free stress
cargo run --release --bin fsdemo             # FULL STACK: concurrent FS ops + snapshots + crash recovery
cargo run --release --bin fsdemo -- 8 200000 # [threads] [ops/thread]
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

## Real lock-free layer (`lockfree.rs`)

`conc.rs` (above) *models* concurrency to validate the wait-free-write bound
without unsafe. `lockfree.rs` is the **real thing**: a multi-threaded, lock-free
copy-on-write radix map driven by atomics, exercised by `cargo run --bin stress`
with real OS threads. It is the production-shaped realization of the CoW-radix
candidate from the design exploration.

- **Wait-free reads** — atomic `Arc` load of a shard root, then walk immutable
  nodes. Never blocks/retries/helps.
- **Lock-free writes** — path-copy the touched path, atomically swap the shard
  root (`ArcSwap::rcu`); retry only on a same-shard race.
- **Reclamation = `Arc` refcounting** — no epochs / hazard pointers / `unsafe`.
- **O(shards) snapshots** — capture each shard's root `Arc`; fully immutable.
- **Sharding** (hash → independent radix trees) drives down cross-key contention;
  global order is recovered by merging per-shard range scans.

Measured (16-core box, `stress 8 300000 256`):

| test | result |
|---|---|
| disjoint write storm | 2.4M writes @ ~1.4M writes/s (8 threads); **1.4%** CAS retries; 0 final-state mismatches |
| single-hot-key contention | ~2× retries/write (the serialization the analysis predicted) — but **0** torn/garbage reads |
| snapshot isolation under 8 overwriters | **0** snapshot drift (CoW immutability holds) |

**Scope:** writes here are **lock-free, not wait-free** — a same-shard race
retries. The hot-key test deliberately shows that retry tail. The **wait-free
write path** that bounds it is the next module.

## Wait-free write layer (`waitfree.rs`)

`WaitFreeRadixMap` makes the **write** path wait-free too — the open problem the
exploration flagged. It was designed by a multi-agent design+adversarial-verify
workflow: four candidate protocols were proposed and attacked; the two survivors
converged on **per-shard flat combining** (Kogan–Petrank fast-path/slow-path)
with the two fixes the review forced:

- **Gate the fast path** — a per-shard `pending` counter; once any writer
  announces, all others route through the combiner. Closes the starvation hole
  (an unbounded stream of bare fast CASes could otherwise starve an announced
  writer forever).
- **Seq-stamped monotone apply** — values store `(op_seq, value)`; a write
  applies only if its seq is newer. Makes helping idempotent and closes the
  lost-update / double-fold holes.

A write costs `O(K)` fast attempts + `O(P)` help rounds (each `O(P·KEY_LEN)`):
a hard `O(K + P)` bound independent of contention *duration* — no schedule can
starve a writer. Still `unsafe`-free (arc-swap + std atomics), Arc reclamation.

Measured (`stress 8 300000 256`):

| | result |
|---|---|
| disjoint write storm | 2.4M writes @ ~1.3M/s; **2,399,801 / 2,400,000 fast-path wins** (≈0 tax uncontended) |
| hot-key, 240k ops on ONE key, 8 writers | **max help-rounds/op = 4** — a small constant (the wait-free witness) |
| vs lock-free on the same hot key | **532,307 retries** (grows with contention) — the tail wait-free bounds |

The `wf_hot_key_no_torn_and_bounded_help_rounds` test asserts the per-op
help-round count stays a small constant over ~96k contended ops.

### Loom model-check (`tests/loom_waitfree.rs`)

The gate+combine core is **machine-verified with [loom]** — exhaustive
interleaving exploration under the C11 memory model. (`arc-swap` can't be
loom-checked, so the test re-expresses the protocol core in loom atomics:
single shard, single contended key, N writers with distinct seqs.) Loom proves,
over every interleaving:

- **safety** — the max-seq write always wins (no lost update / double-fold — FIX 2);
- **completion** — every writer finishes (loom flags deadlock; the in-loop
  `rounds <= 2N+5` assert turns any livelock into a failure);
- **bounded rounds** — a slow-path writer commits within `2N+5` rounds.

Verified: `N=2` exhaustive (both pure-combine `k=0` and gated fast/slow `k=1`),
`N=3` with `LOOM_MAX_PREEMPTIONS=2`. The asymptotic `O(K+P)` starvation-free
bound stays an analytical argument that loom's small-instance proofs corroborate.

```sh
RUSTFLAGS="--cfg loom" cargo test --release --test loom_waitfree -- _2      # exhaustive (N=2)
RUSTFLAGS="--cfg loom" LOOM_MAX_PREEMPTIONS=2 cargo test --release --test loom_waitfree loom_pure_combine_3
```

(loom is a `cfg(loom)`-gated dependency; normal `cargo test` never compiles it.)

[loom]: https://github.com/tokio-rs/loom

## Full synthesis stack (`concurrent.rs`)

`ConcFs` composes the verified pieces into the design the exploration arrived at —
**one stack**:

- **storage** = the wait-free radix map (`waitfree.rs`);
- **snapshots** = in-key snapshot ids + a lock-free ancestry registry; reads
  resolve the visible ancestor version (snapshot-consistent, not live-linearizable);
  **O(1) lock-free `delete_snapshot`** (mark-dead → versions invisible at once) +
  **GC via a per-snapshot dirty-set** (reclaims a dead snapshot's keys ∝ what it
  wrote, not the whole fs — fixes the O(1)-create / O(scan)-delete asymmetry);
- **durability** = a **lock-free per-core journal** (per-thread Treiber log, no
  lock on append); recovery replays the merged, seq-ordered log; the DRAM index
  is authoritative, durability is the separate journal (the move that dissolves
  the wait-free-read vs durable-linearizability collision).

One op-sequence drives both the map's monotone apply and the journal record, so
**`recover()` reconstructs exactly the live state** — the capstone tests assert
`recovered.get == live.get` after a concurrent run, which simultaneously witnesses
**crash consistency** and **linearizability** (the concurrent run equals its own
seq-order serialization).

**Sharding for locality.** Keys shard by their **8-byte inode prefix**, not the
whole key. So every key of one inode (all offsets, all snapshots) lives in one
shard → per-inode point reads and range scans are single-shard/local, while
different inodes spread for write concurrency. (Sharding by the *full* key instead
made every band scan touch all shards — measured at ~50× slower; running the demo
caught it. FS access is per-inode, so inode-prefix sharding is the right fit.)

Measured (`fsdemo 8 200000`, 16 cores): 1.6M mixed ops (put/delete/snapshot/read)
@ ~1.2 M ops/s with ~48k live snapshots; crash → replay 1.09M journal records →
**0 recovered-vs-live mismatches** over 200k sampled reads.

Concurrency of the whole stack: reads wait-free, writes wait-free, snapshot
create lock-free, recovery single-threaded.

## Design notes & honest stubs

- **`conc.rs` is a *model*; `lockfree.rs` is real.** `conc.rs` advances each
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
src/conc.rs       step-level concurrency MODEL: wait-free writers, contention contrast
src/lockfree.rs   REAL lock-free CoW radix map (atomics via arc-swap): wait-free reads,
                  lock-free sharded writes, Arc-refcount reclamation, O(shards) snapshots
src/waitfree.rs   REAL wait-free-WRITE CoW radix map: per-shard flat combining
                  (gated fast path + announce/help + seq-stamped monotone apply);
                  prefix-sharding for per-inode scan locality
src/concurrent.rs FULL STACK: wait-free map + in-key snapshots (lock-free ancestry)
                  + journal durability + snapshot-consistent reads/range (ConcFs)
src/sim.rs        simulator: workload + differential oracle + crash test + concurrency model
src/bin/simulate.rs   sequential-core + concurrency-model report
src/bin/stress.rs     REAL multi-threaded lock-free + wait-free stress + throughput report
src/bin/fsdemo.rs     FULL STACK demo: concurrent FS ops + snapshots + crash recovery
tests/correctness.rs  sequential end-to-end tests
tests/concurrent.rs   real-threads lock-free & wait-free correctness + bounded-help-rounds
tests/stack.rs        full-stack: concurrent workload -> crash -> recover == live
tests/loom_waitfree.rs  loom model-check of the wait-free gate+combine core (cfg(loom))
```
