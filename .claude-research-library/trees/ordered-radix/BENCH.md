# Backbone decision — numbers: ordered-radix (CoW) vs `std::BTreeMap`

Reply-driven follow-up #3 ("decide the backbone with numbers"). FS-shaped
metadata workload: keys `(dir_inode:u64, h64:u64, cd:u16)` = 18B big-endian;
4 000 dirs × 64 dirents = 256 000 keys. `cargo run --release --bin bench`
(16-core box, single-threaded; range counts via a non-allocating visitor on both
sides for a fair count-only comparison).

| operation | ordered-radix (CoW) | std::BTreeMap | radix / bt |
|---|--:|--:|--:|
| bulk insert | 0.65 M/s | 7.98 M/s | **0.08×** |
| dirent lookup | 0.96 M/s | 4.38 M/s | **0.22×** |
| dir range (listing) | 0.04 M dirs/s | 2.55 M dirs/s | **0.02×** |
| **snapshot** (per op) | **2.4 ns** | 7 279 343 ns | **~3 000 000×** |
| memory | **9.9 nodes/key** | (B-tree, B=6) | — |

## Honest reading

**On raw single-thread ops, the naive CoW byte-radix loses to a B-tree by 5–50×**
(insert, lookup, range). **It wins one axis, decisively: O(1) snapshots** — an
`Arc`-clone (~2 ns) vs BTreeMap's only snapshot, a deep `clone()` (O(n), ~7 ms
here). That is the whole point of CoW.

Why the radix loses on raw ops *as built*:
1. **No path compression (not ART).** Keys are 18 B → the trie is up to 18 levels
   deep with **9.9 nodes/key**; every lookup is ~18 pointer-chases vs a B-tree's
   ~7 cache-friendly node touches. This is the dominant factor.
2. **CoW insert cost.** Path-copy clones a children `Vec` at each level per
   insert — inherent to persistence (a non-CoW mutable map inserts far faster but
   has no cheap snapshot).
3. **Hashed dirent keys are radix-hostile for range.** Random `h64` values under
   one inode form a deep, sparse subtree, so listing 64 dirents walks many
   scattered nodes; a B-tree keeps them in a few contiguous nodes.

## What this means for the backbone choice

- **Do not pick a *naive* byte-radix for throughput.** The reason to choose radix
  is **snapshots (O(1)) + lock-free/wait-free concurrency + bounded depth +
  simpler (no-SMO) verification** — not single-thread op speed.
- **The fair next comparison is an ART** (path-compressed + adaptive
  Node4/16/48/256), which the reply itself flagged for on-disk node space
  efficiency. Path compression collapses the 8-byte inode prefix to ~1 node and
  cuts depth toward `O(log fanout)`, which should close most of the lookup/range
  /memory gap **while keeping O(1) snapshots**. Until that exists, these numbers
  understate the radix backbone's potential.
- **Caveat on the baseline.** `std::BTreeMap` is a *non-CoW* B-tree. The reply
  asked for radix vs a *CoW* B+tree; a CoW B+tree would pay a similar path-copy
  insert cost (narrowing the 0.08× insert gap) and would need its own snapshot
  scheme (it would not match radix's free `Arc`-clone snapshot, but could beat
  BTreeMap's full clone). So against the *real* competitor the insert and
  snapshot columns both move in radix's favor; lookup/range still need ART.

**Bottom line for the decision:** radix's measured edge is snapshots +
concurrency + verifiability, not raw ops. Recommend an **ART CoW variant** as the
deciding experiment before committing the on-disk backbone; the naive map here is
the correctness/concurrency reference, not the performance candidate.

---

# ART-CoW results — backbone decision closed

`cargo run --release --bin bench` (16-core box, single-thread, single-run
indicative; same 256k-key FS workload). Adds the path-compressed **Adaptive
Radix Tree** (`ArtCowMap`) as the real radix candidate.

| operation | ART-CoW | naive-radix | std::BTreeMap |
|---|--:|--:|--:|
| insert | 0.89 M/s | 0.65 M/s | 7.42 M/s |
| lookup | **3.89 M/s** | 0.88 M/s | 4.07 M/s |
| dir-range (listing) | 0.135 M dir/s | 0.037 M dir/s | 1.985 M dir/s |
| snapshot (per op) | **2.4 ns (O(1))** | 2.5 ns | 7.9 ms |

ART metrics (the constraint criteria):
| | ART-CoW | naive-radix |
|---|--:|--:|
| nodes/key | **1.12** | 9.90 |
| bytes/key (approx) | 88.7 | — |
| depth (node-hops) | **max 6, avg 4.22** | 18 (fixed) |
| node types | N4=26 791 · N16=7 · N48=21 · N256=3 995 | — |

## Reading (decision input)

- **Lookup gap CLOSED**: 0.88 → 3.89 M/s = **~0.96× of BTreeMap**. Path
  compression + adaptive wide nodes make radix lookups competitive.
- **Memory 8.8× better**: 9.9 → **1.12 nodes/key** — directly helps on-disk size
  and firmware RAM.
- **Depth nailed**: **max 6 / avg 4.22** node-hops (vs naive 18; under the ~10
  target) — the bounded-latency criterion holds. The shape is as predicted:
  the inode prefix compresses, then a wide **N256** node per directory carries
  the ~64-way `h64` fan-out (3 995 N256 ≈ one per dir), N4 below.
- **Snapshot stays O(1)**: 2.4 ns Arc-clone, no regression — the reason to use
  radix is preserved.
- **Remaining weak spots**: (1) **insert** 0.89 vs 7.42 M/s — the CoW path-copy +
  node-clone cost (inherent to persistence; vs a *CoW* B+tree it would be
  comparable, not 8×). (2) **dir-range** still ~15× behind BTreeMap, because
  hashed `h64` dirents are scattered with no shared prefix — but the absolute
  cost is ~7 µs to list a 64-entry directory, acceptable for dir enumeration.

## Verdict

ART-CoW makes **radix a viable on-disk backbone**: competitive lookups, far less
memory, bounded shallow depth, and O(1) snapshots — the combination a comparison
B-tree cannot match (no cheap snapshot) and the naive radix could not deliver on
raw ops. The open trade is insert throughput (CoW cost, comparable to a CoW
B+tree) and dir-listing (intrinsic to hashed keys). Recommend radix(ART) as the
backbone candidate subject to the on-disk node-encoding / firmware-parsability
work (A3-3) being designed in from the start.
