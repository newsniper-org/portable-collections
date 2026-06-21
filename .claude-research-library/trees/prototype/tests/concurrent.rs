//! Real-threads correctness tests for the lock-free CoW radix map.
//! These invariants must hold under *any* interleaving.

use std::sync::{Arc, Barrier};
use std::thread;

use radix_fs_prototype::key::encode;
use radix_fs_prototype::lockfree::LockFreeRadixMap;
use radix_fs_prototype::store::Value;
use radix_fs_prototype::waitfree::WaitFreeRadixMap;

#[test]
fn disjoint_writes_no_lost_updates() {
    let threads = 8;
    let per = 20_000u64;
    let map = Arc::new(LockFreeRadixMap::new(64));
    let barrier = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let map = Arc::clone(&map);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for i in 0..per {
                    map.put(&encode(t as u64, i, 1), Value::Inode(i));
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    // Disjoint keys => deterministic final state: every (t, i) -> Inode(i).
    for t in 0..threads {
        for i in 0..per {
            assert_eq!(map.get(&encode(t as u64, i, 1)), Some(Value::Inode(i)));
        }
    }
}

#[test]
fn hot_key_never_corrupts_and_ends_valid() {
    let threads = 8;
    let per = 10_000u64;
    let map = Arc::new(LockFreeRadixMap::new(16));
    let key = encode(1, 1, 1);
    let barrier = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let map = Arc::clone(&map);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for i in 0..per {
                    map.put(&key, Value::Inode(t as u64 * 1_000_000 + i));
                    // Every observed value must be one that was actually written.
                    if let Some(Value::Inode(v)) = map.get(&key) {
                        let tt = v / 1_000_000;
                        let ii = v % 1_000_000;
                        assert!((tt as usize) < threads && ii < per, "torn/garbage read: {v}");
                    }
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    // Final value must be a real written value (last-writer-wins).
    match map.get(&key) {
        Some(Value::Inode(v)) => {
            assert!((v / 1_000_000) < threads as u64 && (v % 1_000_000) < per);
        }
        other => panic!("unexpected final value: {other:?}"),
    }
}

#[test]
fn snapshot_isolated_under_concurrent_writes() {
    let threads = 6;
    let n = 2_000u64;
    let map = Arc::new(LockFreeRadixMap::new(32));
    for i in 0..n {
        map.put(&encode(1, i, 1), Value::Inode(i));
    }
    let snap = map.snapshot();
    let barrier = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let map = Arc::clone(&map);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for i in 0..n {
                    map.put(&encode(1, i, 1), Value::Inode(i + 9_000_000));
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    // The snapshot taken before the writes must be completely unchanged.
    for i in 0..n {
        assert_eq!(snap.get(&encode(1, i, 1)), Some(Value::Inode(i)));
    }
    // The live map reflects the overwrites.
    for i in 0..n {
        assert_eq!(map.get(&encode(1, i, 1)), Some(Value::Inode(i + 9_000_000)));
    }
}

#[test]
fn concurrent_range_is_consistent() {
    let threads = 4;
    let map = Arc::new(LockFreeRadixMap::new(16));
    let barrier = Arc::new(Barrier::new(threads + 1));
    let writers: Vec<_> = (0..threads)
        .map(|t| {
            let map = Arc::clone(&map);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for i in 0..5_000u64 {
                    map.put(&encode(9, t as u64 * 5_000 + i, 1), Value::Inode(i));
                }
            })
        })
        .collect();
    barrier.wait();
    // Concurrent range scans must always return sorted, well-formed results.
    for _ in 0..50 {
        let r = map.range_inclusive(&encode(9, 0, 0), &encode(9, u64::MAX, u32::MAX));
        for w in r.windows(2) {
            assert!(w[0].0 < w[1].0, "range must stay sorted under concurrency");
        }
    }
    for h in writers {
        h.join().unwrap();
    }
}

// ===================== wait-free write map =====================

#[test]
fn wf_disjoint_writes_no_lost_updates() {
    let threads = 8;
    let per = 20_000u64;
    let map = Arc::new(WaitFreeRadixMap::new(64, threads));
    let barrier = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let map = Arc::clone(&map);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for i in 0..per {
                    map.put(t, &encode(t as u64, i, 1), Value::Inode(i));
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    for t in 0..threads {
        for i in 0..per {
            assert_eq!(map.get(&encode(t as u64, i, 1)), Some(Value::Inode(i)));
        }
    }
}

#[test]
fn wf_hot_key_no_torn_and_bounded_help_rounds() {
    let threads = 8;
    let per = 12_000u64;
    let map = Arc::new(WaitFreeRadixMap::new(16, threads));
    let key = encode(1, 1, 1);
    let barrier = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let map = Arc::clone(&map);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for i in 0..per {
                    map.put(t, &key, Value::Inode(t as u64 * 1_000_000 + i));
                    if let Some(Value::Inode(v)) = map.get(&key) {
                        assert!((v / 1_000_000) < threads as u64 && (v % 1_000_000) < per, "torn read: {v}");
                    }
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    // The wait-free witness: even under max contention over ~96k ops, the
    // worst-case per-op help-round count stays a small CONSTANT (bounded by the
    // O(P) argument), NOT growing with the number of ops. (Lock-free retry would
    // grow.) The bound is generous to stay robust across real OS schedulers.
    let maxr = map.max_help_rounds();
    assert!(maxr >= 1, "expected some slow-path contention");
    assert!(
        maxr <= 64,
        "wait-free per-op help-rounds should be a small constant, got {maxr} over {} ops",
        threads as u64 * per
    );
    // Final value is a real written value (last-by-seq wins).
    match map.get(&key) {
        Some(Value::Inode(v)) => assert!((v / 1_000_000) < threads as u64 && (v % 1_000_000) < per),
        other => panic!("bad final: {other:?}"),
    }
}

#[test]
fn wf_snapshot_isolated_under_concurrent_writes() {
    let threads = 6;
    let n = 2_000u64;
    let map = Arc::new(WaitFreeRadixMap::new(32, threads));
    for i in 0..n {
        map.put(0, &encode(1, i, 1), Value::Inode(i));
    }
    let snap = map.snapshot();
    let barrier = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let map = Arc::clone(&map);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for i in 0..n {
                    map.put(t, &encode(1, i, 1), Value::Inode(i + 9_000_000));
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    for i in 0..n {
        assert_eq!(snap.get(&encode(1, i, 1)), Some(Value::Inode(i)));
    }
}
