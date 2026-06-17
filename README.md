# portable-collections

A small Cargo **workspace** of portable, `no_std`-friendly, dependency-light,
generic data structures. Every crate is `no_std` + `alloc` by default and
`unsafe`-free by policy (`[workspace.lints] unsafe_code = "forbid"` +
`#![forbid(unsafe_code)]`).

The unifying theme is **scoped rollback**: most structures here remember a
`checkpoint()` and can be atomically rewound to it with `rollback_to(mark)`, so a
"primary store ↔ undo ledger" pair cannot drift out of sync. (The bimap was
extracted from a real SMT-solver soundness fix; see `CLAUDE.md`.)

## Conventions

- **`no_std` + `alloc` is the default.** The `std` feature (off by default) only
  adds `std::error::Error` impls — the structures themselves never need `std`.
- **`unsafe`-free**, **generic** (no app-specific key/value types), and **zero
  runtime dependencies** so far (`BTreeMap`/`Vec`/`VecDeque`-backed, `alloc` only).
- **Naming:** concrete types are `<Backend><Trait>` — e.g. `BTreeBimap` /
  `FlatRadixBimap` implement `Bimap`; `VecScopedStack` / `ArrayScopedStack`
  implement `ScopedStack`; `DequeScopedQueue` implements `ScopedQueue`.

## Feature tiers

Validate across all of them — each is a real target:

| tier | invocation | what it adds |
|------|------------|--------------|
| bare `no_std` | `--no-default-features` | `core` only, no allocator (`ArrayScopedStack`, `DenseIndex`, and the trait vocabulary) |
| `alloc` *(default)* | *(none)* | `BTreeMap`/`Vec`/`VecDeque`-backed types, the `Map`/`Set` facade |
| `std` | `--features std` | the `alloc` set + `std::error::Error` impls |
| `unstable` | `--features unstable` | nightly-only extras (gated via a build-script channel probe) |

## Crates

| crate | provides | status |
|-------|----------|--------|
| [`portable-collection-primitives`](portable-collection-primitives/) | the shared **trait vocabulary** — `Container`, `ScopedRollback` (+ `Checkpoint`), `Bimap`, `ScopedStack`/`ScopedQueue`, `Push`/`TryPush`/`Pop`/`Pull`, and the alloc-tier `Map`/`MapShim`/`Set`/`SetShim` facade — plus the cfg/codegen macros (`ifstd!`/`ifalloc!`/`ifstdoralloc!`, `group!`/`implgroup_for!`, `wrap_into_map_traits!`/`wrap_into_set_traits!`) | traits + macros (3 unit + 6 doc) |
| [`portable-bijectives`](portable-bijectives/) | `BTreeBimap<K, V>` and `FlatRadixBimap<K, V>` — 1-to-1 `K ↔ V` maps with insertion order and **atomic scope rollback** (`+ InsertError`, `DenseIndex`) | implemented + tested (18 unit + 2 doc) + criterion bench |
| [`portable-queues`](portable-queues/) | scoped append-logs: `VecScopedStack` / `ArrayScopedStack` (LIFO `ScopedStack`) and `DequeScopedQueue` (FIFO `ScopedQueue`) — `checkpoint`/`rollback_to`/`drain_since` over a scope stack | implemented + tested (17 unit + 1 doc) |
| [`portable-maps-and-sets`](portable-maps-and-sets/) | concrete `Map`/`Set` implementations on the primitives facade | scaffolded (no impls yet) |

### Trait hierarchy

```
Container  (len / is_empty / clear)
└─ ScopedRollback  (checkpoint() → Mark ;  rollback_to(Mark))
   ├─ Bimap<K, V>            → BTreeBimap, FlatRadixBimap
   ├─ ScopedStack<T>  (: Push + Pop)   → VecScopedStack, ArrayScopedStack
   └─ ScopedQueue<T>  (: Push + Pull)  → DequeScopedQueue

Push<T> / TryPush<T> / Pop<T> (LIFO) / Pull<T> (FIFO)   — the access traits
Map / Set (+ MapShim / SetShim Borrow shim)             — alloc-tier facade
```

## Build / test

```sh
cargo test                          # default: no_std + alloc
cargo test --features std           # adds std::error::Error impls
cargo build --no-default-features   # bare no_std (core only)
cargo build --features unstable     # nightly extras
```

See `CLAUDE.md` for the full design rationale, the downstream consumer (an SMT
solver's arithmetic-theory interner), the research library behind the backend
choices, and the roadmap — it is written as a handoff for a fresh session.
