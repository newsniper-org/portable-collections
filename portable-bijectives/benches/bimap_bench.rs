//! Criterion benchmark: `BTreeBimap` (tree-backed) vs `FlatRadixBimap` (dense
//! `Vec` radix) on the interner's three hot operations over dense `u32` ids —
//! bulk insert, point lookup, and scoped push/pop rollback.
//!
//! This is the research survey's go/no-go gate: `FlatRadixBimap` must win the
//! dense workload to justify itself over the already-cache-conscious `BTreeMap`
//! backing `BTreeBimap`. Run with `cargo bench -p portable-bijectives`.

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use portable_bijectives::{BTreeBimap, FlatRadixBimap};

const N: u32 = 10_000;

fn bench_insert(c: &mut Criterion) {
    let mut g = c.benchmark_group("insert_dense");
    g.bench_function("BTreeBimap", |b| {
        b.iter(|| {
            let mut m: BTreeBimap<u32, u32> = BTreeBimap::new();
            for i in 0..N {
                m.insert(black_box(i), black_box(i)).unwrap();
            }
            m
        });
    });
    g.bench_function("FlatRadixBimap", |b| {
        b.iter(|| {
            let mut m: FlatRadixBimap<u32, u32> = FlatRadixBimap::new();
            for i in 0..N {
                m.insert(black_box(i), black_box(i)).unwrap();
            }
            m
        });
    });
    g.finish();
}

fn bench_lookup(c: &mut Criterion) {
    let mut bt: BTreeBimap<u32, u32> = BTreeBimap::new();
    let mut fr: FlatRadixBimap<u32, u32> = FlatRadixBimap::new();
    for i in 0..N {
        bt.insert(i, i).unwrap();
        fr.insert(i, i).unwrap();
    }
    let mut g = c.benchmark_group("lookup_dense");
    g.bench_function("BTreeBimap", |b| {
        b.iter(|| {
            let mut acc = 0u64;
            for i in 0..N {
                acc += u64::from(*bt.get(black_box(&i)).unwrap());
            }
            acc
        });
    });
    g.bench_function("FlatRadixBimap", |b| {
        b.iter(|| {
            let mut acc = 0u64;
            for i in 0..N {
                acc += u64::from(*fr.get(black_box(&i)).unwrap());
            }
            acc
        });
    });
    g.finish();
}

fn bench_scoped_rollback(c: &mut Criterion) {
    // Simulate a solver push/pop: from a half-filled map, checkpoint, intern the
    // second half inside the scope, then pop back to the checkpoint.
    let mut g = c.benchmark_group("scoped_rollback");
    g.bench_function("BTreeBimap", |b| {
        b.iter_batched(
            || {
                let mut m: BTreeBimap<u32, u32> = BTreeBimap::new();
                for i in 0..N / 2 {
                    m.insert(i, i).unwrap();
                }
                m
            },
            |mut m| {
                let cp = m.checkpoint();
                for i in N / 2..N {
                    m.insert(black_box(i), black_box(i)).unwrap();
                }
                m.truncate(cp);
                m
            },
            BatchSize::SmallInput,
        );
    });
    g.bench_function("FlatRadixBimap", |b| {
        b.iter_batched(
            || {
                let mut m: FlatRadixBimap<u32, u32> = FlatRadixBimap::new();
                for i in 0..N / 2 {
                    m.insert(i, i).unwrap();
                }
                m
            },
            |mut m| {
                let cp = m.checkpoint();
                for i in N / 2..N {
                    m.insert(black_box(i), black_box(i)).unwrap();
                }
                m.truncate(cp);
                m
            },
            BatchSize::SmallInput,
        );
    });
    g.finish();
}

criterion_group!(benches, bench_insert, bench_lookup, bench_scoped_rollback);
criterion_main!(benches);
