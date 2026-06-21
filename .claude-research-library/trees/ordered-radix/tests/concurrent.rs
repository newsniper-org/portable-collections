//! Real-threads test of the lock-free concurrent map (only with `--features concurrent`).
#![cfg(feature = "concurrent")]

use std::sync::{Arc, Barrier};
use std::thread;

use ordered_radix::concurrent::ConcurrentRadixMap;

fn k(inode: u64, off: u64) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[0..8].copy_from_slice(&inode.to_be_bytes());
    b[8..16].copy_from_slice(&off.to_be_bytes());
    b
}

#[test]
fn disjoint_concurrent_writes() {
    let threads = 8;
    let per = 20_000u64;
    let map = Arc::new(ConcurrentRadixMap::<u64>::new(64, 8)); // shard by inode prefix
    let barrier = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let map = Arc::clone(&map);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for i in 0..per {
                    map.insert(&k(t as u64, i), i);
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    for t in 0..threads as u64 {
        for i in 0..per {
            assert_eq!(map.get(&k(t, i)), Some(i));
        }
    }
}

#[test]
fn snapshot_isolation_under_writes() {
    let threads = 6;
    let n = 2_000u64;
    let map = Arc::new(ConcurrentRadixMap::<u64>::new(32, 8));
    for i in 0..n {
        map.insert(&k(1, i), i);
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
                    map.insert(&k(1, i), i + 9_000_000);
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    for i in 0..n {
        assert_eq!(snap.get(&k(1, i)), Some(i)); // snapshot frozen
        assert_eq!(map.get(&k(1, i)), Some(i + 9_000_000)); // live updated
    }
}

#[test]
fn per_inode_range_is_local_and_ordered() {
    let map = ConcurrentRadixMap::<u64>::new(32, 8);
    for i in [5u64, 1, 9, 3, 7] {
        map.insert(&k(42, i), i);
    }
    map.insert(&k(43, 0), 100); // other inode, must be excluded
    let lo = k(42, 0);
    let hi = k(42, u64::MAX);
    let got: Vec<u64> = map.range(&lo, &hi).into_iter().map(|(_, v)| v).collect();
    assert_eq!(got, vec![1, 3, 5, 7, 9]);
}
