//! First-cut benchmark: `ordered-radix` (CoW persistent) vs `std::BTreeMap`
//! (the in-memory comparison-B-tree baseline) on an **FS-shaped metadata
//! workload** — the reply's request to "decide the backbone with numbers".
//!
//! Keys are `(dir_inode: u64, h64: u64, cd: u16)` = 18 bytes big-endian, the
//! shape filesystem-researches uses for dirents (original name is the value).
//! Workload: bulk insert, dirent point-lookup, per-directory range (listing),
//! and snapshot — the last being where CoW's O(1) snapshot diverges sharply from
//! BTreeMap's O(n) clone.
//!
//! Honest framing: BTreeMap is a B-tree (B=6), not a CoW B+tree, and has no
//! cheap snapshot (clone is its only one). This measures radix-vs-comparison-tree
//! on FS ops + the CoW snapshot advantage. Run: `cargo run --release --bin bench`.

use std::collections::BTreeMap;
use std::time::Instant;

use ordered_radix::{CowRadixMap, OrderedMap, SnapshotMap};

const DIRS: u64 = 4_000; // directories (inodes)
const PER_DIR: u64 = 64; // dirents per directory
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
    // stand-in for keyed name-hash h64
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

fn secs(t: Instant) -> f64 {
    t.elapsed().as_secs_f64()
}

fn main() {
    let total_keys = DIRS * PER_DIR;
    println!("== ordered-radix (CoW) vs std::BTreeMap — FS metadata workload ==");
    println!("keys=(dir_inode,h64,cd) 18B; dirs={DIRS} per_dir={PER_DIR} total={total_keys}\n");

    // Precompute the key set (same for both) so we time the maps, not the RNG.
    let mut keys: Vec<[u8; 18]> = Vec::with_capacity(total_keys as usize);
    for d in 0..DIRS {
        for e in 0..PER_DIR {
            keys.push(key(d, fnv(d * 1_000_003 + e), 0));
        }
    }

    // ---- bulk insert ----
    let t = Instant::now();
    let mut radix: CowRadixMap<u64> = CowRadixMap::new();
    for (i, k) in keys.iter().enumerate() {
        radix.insert(k, i as u64);
    }
    let r_ins = secs(t);

    let t = Instant::now();
    let mut bt: BTreeMap<[u8; 18], u64> = BTreeMap::new();
    for (i, k) in keys.iter().enumerate() {
        bt.insert(*k, i as u64);
    }
    let b_ins = secs(t);

    // ---- dirent point lookup ----
    let mut rng = Rng(1);
    let t = Instant::now();
    let mut acc = 0u64;
    for _ in 0..LOOKUPS {
        let k = &keys[(rng.next() % total_keys) as usize];
        if let Some(v) = radix.get(k) {
            acc ^= *v;
        }
    }
    let r_get = secs(t);

    let mut rng = Rng(1);
    let t = Instant::now();
    for _ in 0..LOOKUPS {
        let k = &keys[(rng.next() % total_keys) as usize];
        if let Some(v) = bt.get(k) {
            acc ^= *v;
        }
    }
    let b_get = secs(t);

    // ---- per-directory range (listing) ----
    // Fair count-only comparison: radix uses the non-allocating visitor, BTreeMap
    // counts its lazy iterator — neither materializes results.
    let mut rng = Rng(7);
    let t = Instant::now();
    let mut rcount = 0usize;
    for _ in 0..RANGES {
        let d = rng.next() % DIRS;
        let lo = key(d, 0, 0);
        let hi = key(d, u64::MAX, u16::MAX);
        radix.for_each_range(&lo, &hi, |_, _| rcount += 1);
    }
    let r_range = secs(t);

    let mut rng = Rng(7);
    let t = Instant::now();
    let mut bcount = 0usize;
    for _ in 0..RANGES {
        let d = rng.next() % DIRS;
        let lo = key(d, 0, 0);
        let hi = key(d, u64::MAX, u16::MAX);
        bcount += bt.range(lo..=hi).count();
    }
    let b_range = secs(t);
    assert_eq!(rcount, bcount, "range results must match");

    // ---- snapshot (the CoW divergence) ----
    const SNAPS: u64 = 100_000;
    let t = Instant::now();
    let mut keep = 0usize;
    for _ in 0..SNAPS {
        let s = radix.snapshot();
        keep = keep.wrapping_add(s.len());
    }
    let r_snap = secs(t);

    // BTreeMap has no cheap snapshot: clone() is the only one (O(n)). Do far
    // fewer to keep wall-time sane, then normalize per-snapshot.
    const BT_SNAPS: u64 = 200;
    let t = Instant::now();
    for _ in 0..BT_SNAPS {
        let c = bt.clone();
        keep = keep.wrapping_add(c.len());
    }
    let b_snap_per = secs(t) / BT_SNAPS as f64;
    let r_snap_per = r_snap / SNAPS as f64;

    // ---- report ----
    let mops = |n: u64, s: f64| n as f64 / s / 1e6;
    println!("operation            ordered-radix         std::BTreeMap        radix/bt");
    println!("-------------------- --------------------- -------------------- --------");
    println!("bulk insert          {:>7.2} M/s          {:>7.2} M/s          {:>5.2}x", mops(total_keys, r_ins), mops(total_keys, b_ins), b_ins / r_ins);
    println!("dirent lookup        {:>7.2} M/s          {:>7.2} M/s          {:>5.2}x", mops(LOOKUPS, r_get), mops(LOOKUPS, b_get), b_get / r_get);
    println!("dir range (listing)  {:>7.2} M dirs/s     {:>7.2} M dirs/s     {:>5.2}x", mops(RANGES, r_range), mops(RANGES, b_range), b_range / r_range);
    println!("snapshot (per op)    {:>9.1} ns           {:>9.1} ns           {:>7.0}x", r_snap_per * 1e9, b_snap_per * 1e9, b_snap_per / r_snap_per);
    println!("\nmemory: radix nodes = {} ({:.2} nodes/key)", radix.node_count(), radix.node_count() as f64 / total_keys as f64);
    println!("(ratio > 1 = ordered-radix faster; snapshot column shows CoW's O(1) vs BTreeMap clone O(n))");
    std::hint::black_box((acc, keep, rcount));
}
