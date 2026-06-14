# portable-collections — session handoff

This file is written for a **fresh Claude Code session** picking up work here.
It carries the context that is not obvious from the code alone.

## What this is

`~/portable-collections/` is a Cargo workspace of **portable, `no_std`-friendly,
dependency-light, generic data structures**. It was extracted so the structures
can be developed and tested in isolation, then depended on by other projects
(notably an SMT solver — see *Downstream consumer*).

Members:
- **`portable-bijectives`** — the only crate so far. Exports `BTreeBimap<K, V>`.

The name is plural on purpose: expect to add more collections here over time.

### Hard conventions (keep these)
- **`no_std` + `alloc` is the default.** A `std` feature (off by default) only
  adds `std::error::Error` impls; the structures themselves never need `std`.
  Validate both: `cargo test` and `cargo test --features std`.
- **Generic.** No app-specific key/value types baked in.
- **`unsafe`-free** by policy (`[workspace.lints] unsafe_code = "forbid"`, and
  `#![forbid(unsafe_code)]` in the crate).
- **Zero runtime deps** so far (BTreeMap-backed, `alloc` only). If a future
  collection needs a hash map in `no_std`, prefer the `hashbrown` crate behind a
  feature rather than forcing it on everyone.
- **Do NOT invent copyright-holder names.** The `license` field is fine to set
  (`Apache-2.0 OR MIT`), but ask the user before adding SPDX `FileCopyrightText`
  headers with a name. `authors = []` for now.
- **Naming:** the type is `BTreeBimap` (a BTree-backed bimap), chosen by the user
  over `ScopedBimap`. Match that style for new types (backend-prefixed if useful,
  e.g. a future `HashBimap`).

## `BTreeBimap<K, V>` — what it is and **why it exists**

A bijection `K ↔ V` (one key ↔ one value, both directions are lookups) that also
remembers **insertion order** and can be **rolled back to a checkpoint** in a
single atomic call. Backed by two `alloc::collections::BTreeMap`s (`fwd`, `rev`)
plus a `Vec<(K, V)>` order/rollback log. `K: Ord + Clone`, `V: Ord + Clone`.

API: `new` (const), `len`/`is_empty`, `get`/`get_key`, `contains_key`/
`contains_value`, `insert` (→ `Result<(), InsertError>`, refuses to break
bijectivity), `checkpoint` (= current len), `truncate(n)` (atomic rollback),
`clear`, `iter`/`keys`/`values` (insertion order), `entries`.

**It exists to make one specific bug class *unrepresentable*.** It is the
structural form of a real soundness fix:

> An SMT arithmetic-theory solver kept two parallel maps — `term_to_var`
> (`TermId → VarId`, a `HashMap`) and `var_to_term` (intern-order `Vec<TermId>`).
> Its `pop()` (scope exit) truncated `var_to_term` and popped the simplex but
> **forgot to roll back `term_to_var`**. A term interned in the popped scope kept
> a stale `VarId` the simplex no longer had; a later `intern()` returned it, and
> the next pivot indexed the simplex arrays out of bounds — a **hard panic** on
> otherwise-valid `(push)`/`(pop)` input. (The manual fix landed first; this type
> replaces both maps so a one-vs-other desync cannot be written.)

So when reviewing or extending `truncate`, the invariant to protect is: **every
mutation touches `fwd`, `rev`, and `order` together; rollback removes from all
three.** That is the whole point.

## Downstream consumer (the application is PENDING)

The intended first consumer is **OxiZ's `ArithSolver`** (an SMT theory solver),
in the repo `external/oxiz` of the adsmt project, on branch
**`0.2.4-hybridization`**, file `oxiz-theories/src/arithmetic/solver.rs`.

The plan ("apply" step): replace the `term_to_var` + `var_to_term` fields with a
single `BTreeBimap<TermId, VarId>`:
- `intern(term)`: `if let Some(v) = bimap.get(&term) { return v }` else allocate
  `simplex.new_var()` and `bimap.insert(term, var)`.
- `push` checkpoint: store `bimap.checkpoint()` (the intern count).
- `pop`: `bimap.truncate(state.num_vars)` — **one call**, replaces the manual
  two-map rollback (the bug becomes unrepresentable).
- `reset`: `bimap.clear()`.
- `derive_shared_equalities`: iterate `bimap.iter()` for `(term, var)` pairs.

**Caveat to handle when applying:** the original maps used `HashMap` (`Hash + Eq`);
`BTreeBimap` needs `Ord` on `K` *and* `V`. OxiZ's `TermId`/`VarId` are small id
newtypes — add `#[derive(PartialOrd, Ord)]` if they lack it (harmless, and gives
deterministic iteration, which is good for reproducible solving).

**Status:** the application is on hold. The **user wires the git dependency
manually** (they did not want it auto-wired). Once OxiZ can depend on this crate
(git dep, or vendored), do the replacement above and validate the OxiZ suites
(`oxiz-theories`, `oxiz-solver`) green — especially the push/pop + arith tests
and a prelude-scale multi-`(push)` session (which previously panicked).

## Research library — `~/research-library/data-structures/`

A companion library of **22 papers (PDF) on B-tree improvements** was downloaded
to `~/research-library/data-structures/` (outside this workspace), with a
`README.md` index. Use it when deciding the *backend* for these collections.

The question it answers: **keep the B-tree's strengths** (high fanout → shallow,
cache-friendly contiguous nodes, ordered range scans, near-optimal external-memory
transfers) **while fixing its weaknesses** (in-memory pointer chasing / cache
misses, write amplification, lock contention, hardware-tuned `B`, comparison cost
vs radix/hash). Directions (folder README has the per-paper mapping):

- **A. Cache-obliviousness** — optimal transfers without tuning `B` (Bender–Demaine–
  Farach-Colton Cache-Oblivious B-Trees; Streaming B-trees / COLA).
- **B. Write-optimization** — Bε-trees (TokuDB/BetrFS), LSM-tree (the alternative).
- **C. Cache-conscious main-memory layout** — CSB+-trees (contiguous children →
  ~2× fanout), Masstree (trie of B+-trees), SIMD/cache index search.
- **D. Concurrency / lock-freedom** — Bw-tree (+ the candid CMU "…Buzz Words"
  critique), OLFIT, Optimistic Lock Coupling / ROWEX.
- **E. Persistent / heterogeneous memory** — NVM/SCM allocation, CXL tiered memory.
- **F. Alternative / adaptive structures** — ART (Adaptive Radix Tree): radix (not
  comparison) search, adaptive node sizes, cache-aware, space-efficient.

**Most relevant to this workspace = C and F.** Rust's `BTreeMap` (which
`BTreeBimap` wraps) is `B = 6` with linear in-node scan, cache-oriented. For an
interner-style consumer (heavy point lookups, dense small-integer ids, scoped
push/pop), the promising backends to explore are:
- **C**: a cache-conscious node layout (CSB+-style) if we ever hand-roll a map.
- **F**: an **ART/radix-backed bimap** — dense small ids are an ideal radix key,
  and ART tends to beat comparison trees on exactly this workload. A
  `RadixBimap`/`ArtBimap` member (same `checkpoint`/`truncate` contract) is the
  natural next experiment, benchmarked against `BTreeBimap`.

A/B/E matter only for external-memory / write-heavy / persistent settings, which
this workspace does not currently target — note them but don't over-invest.

## Roadmap / open ideas

1. **Apply `BTreeBimap` to OxiZ `ArithSolver`** (pending the user's git wiring) —
   see *Downstream consumer*. This is the immediate next step.
2. **`HashBimap`** behind a `hashbrown` feature, for `Hash`-keyed consumers that
   can't provide `Ord` (keeps the same scope-rollback contract).
3. **`RadixBimap` / ART-backed** variant for dense small-integer keys (research
   dir F); benchmark vs `BTreeBimap` on an interner workload.
4. **Benchmarks** (criterion, std-only dev-dep) comparing backends on
   insert / lookup / scoped-rollback patterns.
5. Grow the workspace with other portable collections as needs arise.

## Build / test / git

- `cargo test` (no_std default) and `cargo test --features std` — both must pass.
- `cargo build` builds `no_std`; the only `std::` use is the feature-gated
  `std::error::Error` impl.
- **Git:** the repo is initialized here; the **user handles the remote and any
  push/wiring**. Don't add a remote or push without being asked.
