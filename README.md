# portable-collections

A small Cargo **workspace** of portable, `no_std`-friendly, dependency-light
generic data structures. Every crate is `#![no_std]` + `alloc` by default and
`unsafe`-free by policy (`[workspace.lints] unsafe_code = "forbid"`).

## Crates

| crate | what | status |
|-------|------|--------|
| [`portable-bijectives`](portable-bijectives/) | `BTreeBimap<K, V>` — a 1-to-1 `K ↔ V` map with insertion order and **atomic scope rollback** (`checkpoint` / `truncate`) | implemented + tested (9 unit + 1 doctest) |

## Build / test

```sh
cargo test                 # default: no_std + alloc
cargo test --features std  # adds std::error::Error impls
```

See `CLAUDE.md` for the full design rationale, the downstream consumer (an SMT
solver's arithmetic-theory interner), the relevant research library, and the
roadmap — it is written as a handoff for a fresh session.
