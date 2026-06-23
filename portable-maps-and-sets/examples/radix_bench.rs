//! Backbone-decision benchmark (filesystem-researches greenlight criteria):
//! **ART-CoW vs naive-radix vs `std::BTreeMap`** on an FS metadata workload,
//! measuring not just throughput but the constraints that close the decision:
//! memory (nodes/key + bytes/key), depth (max & avg — bounded-latency), the
//! separate dir-listing (range) cost, and that the CoW O(1) snapshot survives.
//!
//! Keys `(dir_inode:u64, h64:u64, cd:u16)` = 18B big-endian; `h64` is a keyed
//! PRF (random — no shared prefix within a dir), so path-compression only helps
//! the inode prefix and the `h64` depth must come from adaptive wide nodes.
//! `cargo run --release --example radix_bench`.

use std::collections::BTreeMap;
use std::time::Instant;

use portable_collection_primitives::{Container,MapReadShim, MapRefKeyInsertShim};
use portable_maps_and_sets::radix::{ArtOrderedMap, OrderedMap, RadixOrderedMap, SnapshotMap};

const DIRS: u64 = 4_000;
const PER_DIR: u64 = 64;
const LOOKUPS: u64 = 1_000_000;
const RANGES: u64 = 50_000;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

fn fnv(name: u64) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in name.to_le_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn key(inode: u64, h64: u64, cd: u16) -> [u8; 18] {
    let mut k = [0u8; 18];
    k[0..8].copy_from_slice(&inode.to_be_bytes());
    k[8..16].copy_from_slice(&h64.to_be_bytes());
    k[16..18].copy_from_slice(&cd.to_be_bytes());
    k
}

fn s(t: Instant) -> f64 {
    t.elapsed().as_secs_f64()
}
fn mops(n: u64, secs: f64) -> f64 {
    n as f64 / secs / 1e6
}

fn main() {
    let total = DIRS * PER_DIR;
    println!("== backbone bench: ART-CoW vs naive-radix vs std::BTreeMap — FS metadata ==");
    println!("keys=(dir_inode,h64,cd) 18B; dirs={DIRS} per_dir={PER_DIR} total={total}\n");

    let mut keys: Vec<[u8; 18]> = Vec::with_capacity(total as usize);
    for d in 0..DIRS {
        for e in 0..PER_DIR {
            keys.push(key(d, fnv(d * 1_000_003 + e), 0));
        }
    }

    // ---- insert ----
    let t = Instant::now();
    let mut radix: RadixOrderedMap<u64> = RadixOrderedMap::new();
    for (i, k) in keys.iter().enumerate() {
        radix.insert(k, i as u64);
    }
    let r_ins = s(t);

    let t = Instant::now();
    let mut art: ArtOrderedMap<u64> = ArtOrderedMap::new();
    for (i, k) in keys.iter().enumerate() {
        art.insert(k, i as u64);
    }
    let a_ins = s(t);

    let t = Instant::now();
    let mut bt: BTreeMap<[u8; 18], u64> = BTreeMap::new();
    for (i, k) in keys.iter().enumerate() {
        bt.insert(*k, i as u64);
    }
    let b_ins = s(t);

    // ---- lookup ----
    let mut acc = 0u64;
    let timed_lookup = |m: &dyn Fn(&[u8; 18]) -> Option<u64>| -> f64 {
        let mut rng = Rng(1);
        let t = Instant::now();
        for _ in 0..LOOKUPS {
            if let Some(v) = m(&keys[(rng.next() % total) as usize]) {
                std::hint::black_box(v);
            }
        }
        s(t)
    };
    let r_get = timed_lookup(&|k| radix.get(k).copied());
    let a_get = timed_lookup(&|k| art.get(k).copied());
    let b_get = timed_lookup(&|k| bt.get(k).copied());

    // ---- dir range (listing), count-only (fair) ----
    let mut rc = 0usize;
    let mut ac = 0usize;
    let mut bc = 0usize;
    let mut rng = Rng(7);
    let t = Instant::now();
    for _ in 0..RANGES {
        let d = rng.next() % DIRS;
        radix.for_each_range(&key(d, 0, 0), &key(d, u64::MAX, u16::MAX), |_, _| rc += 1);
    }
    let r_range = s(t);
    let mut rng = Rng(7);
    let t = Instant::now();
    for _ in 0..RANGES {
        let d = rng.next() % DIRS;
        art.for_each_range(&key(d, 0, 0), &key(d, u64::MAX, u16::MAX), |_, _| ac += 1);
    }
    let a_range = s(t);
    let mut rng = Rng(7);
    let t = Instant::now();
    for _ in 0..RANGES {
        let d = rng.next() % DIRS;
        bc += bt.range(key(d, 0, 0)..=key(d, u64::MAX, u16::MAX)).count();
    }
    let b_range = s(t);
    assert_eq!(rc, ac);
    assert_eq!(ac, bc);

    // ---- snapshot ----
    const SNAPS: u64 = 200_000;
    const BT_SNAPS: u64 = 200;
    let t = Instant::now();
    for _ in 0..SNAPS {
        acc = acc.wrapping_add(radix.snapshot().len() as u64);
    }
    let r_snap = s(t) / SNAPS as f64;
    let t = Instant::now();
    for _ in 0..SNAPS {
        acc = acc.wrapping_add(art.snapshot().len() as u64);
    }
    let a_snap = s(t) / SNAPS as f64;
    let t = Instant::now();
    for _ in 0..BT_SNAPS {
        acc = acc.wrapping_add(bt.clone().len() as u64);
    }
    let b_snap = s(t) / BT_SNAPS as f64;

    // ---- report ----
    println!("{:<20} {:>14} {:>14} {:>14}", "operation", "ART-CoW", "naive-radix", "BTreeMap");
    println!("{:-<20} {:->14} {:->14} {:->14}", "", "", "", "");
    println!("{:<20} {:>11.2} M/s {:>11.2} M/s {:>11.2} M/s", "insert", mops(total, a_ins), mops(total, r_ins), mops(total, b_ins));
    println!("{:<20} {:>11.2} M/s {:>11.2} M/s {:>11.2} M/s", "lookup", mops(LOOKUPS, a_get), mops(LOOKUPS, r_get), mops(LOOKUPS, b_get));
    println!("{:<20} {:>9.3} M d/s {:>9.3} M d/s {:>9.3} M d/s", "dir-range(listing)", mops(RANGES, a_range), mops(RANGES, r_range), mops(RANGES, b_range));
    println!("{:<20} {:>11.1} ns {:>11.1} ns {:>11.1} ns", "snapshot (per op)", a_snap * 1e9, r_snap * 1e9, b_snap * 1e9);

    let (amax, aavg) = art.depth_stats();
    let h = art.node_type_histogram();
    println!("\nART metrics (vs naive-radix):");
    println!("  nodes/key : ART {:.2}   |  naive-radix {:.2}", art.node_count() as f64 / total as f64, radix.node_count() as f64 / total as f64);
    println!("  bytes/key : ART {:.1}", art.approx_bytes() as f64 / total as f64);
    println!("  depth     : ART max={amax} avg={aavg:.2} node-hops  (naive-radix = 18, fixed)  [target ~10]");
    println!("  node types: N4={} N16={} N48={} N256={}", h[0], h[1], h[2], h[3]);
    println!("  snapshot  : O(1) preserved (Arc-clone, {a_snap:.1e}s)  vs BTreeMap clone O(n)");
    std::hint::black_box((acc, rc));
    println!("\n(ratio reading: closer ART throughput is to BTreeMap = gap closed; depth ~10 & nodes/key down = bounded-latency + memory win; snapshot stays O(1).)");
}
