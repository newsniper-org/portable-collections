//! Multi-threaded stress test + throughput report for the **real lock-free**
//! CoW radix map (`lockfree::LockFreeRadixMap`).
//!
//! Usage: `cargo run --release --bin stress -- [threads] [ops_per_thread] [shards]`
//!
//! It runs three things with real OS threads:
//!   1. disjoint-key write storm  -> throughput + retry tax + final-state check
//!   2. single-hot-key contention -> exercises the lock-free retry path
//!   3. snapshot isolation under concurrent writes

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

use radix_fs_prototype::key::encode;
use radix_fs_prototype::lockfree::LockFreeRadixMap;
use radix_fs_prototype::store::Value;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let threads: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(8);
    let per: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(200_000);
    let shards: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(256);

    println!("== lock-free radix map stress ==");
    println!("threads={threads} ops/thread={per} shards={shards}\n");

    // ---- 1. disjoint-key write storm ----
    let map = Arc::new(LockFreeRadixMap::new(shards));
    let barrier = Arc::new(Barrier::new(threads));
    let start = Instant::now();
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let map = Arc::clone(&map);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for i in 0..per {
                    // thread t owns inode = t; keys are disjoint across threads.
                    map.put(&encode(t as u64, i, 1), Value::Inode(i));
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let dur = start.elapsed();
    let total = threads as u64 * per;
    let mops = total as f64 / dur.as_secs_f64() / 1.0e6;

    // final-state check: each (t, i) must hold its last written value.
    let mut bad = 0u64;
    for t in 0..threads {
        for i in (0..per).step_by((per / 64).max(1) as usize) {
            if map.get(&encode(t as u64, i, 1)) != Some(Value::Inode(i)) {
                bad += 1;
            }
        }
    }
    println!("1) disjoint write storm");
    println!("   {total} writes in {:.3}s  =>  {mops:.2} M writes/s ({threads} threads)", dur.as_secs_f64());
    println!("   CAS retries (contention tax): {} ({:.4}% of writes)", map.retries(), 100.0 * map.retries() as f64 / total as f64);
    println!("   final-state mismatches: {bad}  [wait-free reads see the committed last write]");

    // ---- 2. single-hot-key contention ----
    let hot = Arc::new(LockFreeRadixMap::new(shards));
    let observed_ok = Arc::new(AtomicU64::new(0));
    let observed_bad = Arc::new(AtomicU64::new(0));
    let hot_key = encode(7, 7, 1);
    let barrier = Arc::new(Barrier::new(threads));
    let hper = (per / 10).max(1);
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let hot = Arc::clone(&hot);
            let barrier = Arc::clone(&barrier);
            let ok = Arc::clone(&observed_ok);
            let badc = Arc::clone(&observed_bad);
            thread::spawn(move || {
                barrier.wait();
                for i in 0..hper {
                    // all threads hammer ONE key (max contention)
                    hot.put(&hot_key, Value::Inode(t as u64 * 1_000_000 + i));
                    // a concurrent reader must only ever observe a *written* value
                    if let Some(Value::Inode(v)) = hot.get(&hot_key) {
                        let tt = v / 1_000_000;
                        let ii = v % 1_000_000;
                        if (tt as usize) < threads && ii < hper {
                            ok.fetch_add(1, Ordering::Relaxed);
                        } else {
                            badc.fetch_add(1, Ordering::Relaxed); // torn / garbage read
                        }
                    }
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    println!("\n2) single-hot-key contention ({} writers, {hper} writes each)", threads);
    println!("   CAS retries: {} ({:.2}x the write count -> the serialization the analysis predicted)", hot.retries(), hot.retries() as f64 / (threads as u64 * hper) as f64);
    println!("   reads observing a real written value: {}  / torn-or-garbage reads: {}", observed_ok.load(Ordering::Relaxed), observed_bad.load(Ordering::Relaxed));

    // ---- 3. snapshot isolation under concurrent writes ----
    let map2 = Arc::new(LockFreeRadixMap::new(shards));
    for i in 0..1000u64 {
        map2.put(&encode(1, i, 1), Value::Inode(i));
    }
    let snap = map2.snapshot();
    let barrier = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let map2 = Arc::clone(&map2);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for i in 0..1000u64 {
                    map2.put(&encode(1, i, 1), Value::Inode(i + 9_000_000)); // overwrite
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let mut snap_bad = 0u64;
    for i in 0..1000u64 {
        if snap.get(&encode(1, i, 1)) != Some(Value::Inode(i)) {
            snap_bad += 1;
        }
    }
    println!("\n3) snapshot isolation under {} concurrent overwriters", threads);
    println!("   snapshot drift: {snap_bad} (must be 0 — CoW snapshot is immutable)");

    let ok = bad == 0 && observed_bad.load(Ordering::Relaxed) == 0 && snap_bad == 0;
    println!("\nRESULT: {}", if ok { "OK — lock-free, safe, snapshot-isolated under real threads." } else { "FAILED" });
    if !ok {
        std::process::exit(1);
    }
}
