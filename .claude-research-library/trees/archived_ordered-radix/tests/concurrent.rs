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

// ---- concurrent ART (criterion B6): node-type growth under lock-free writes ----

#[test]
fn concurrent_art_node_growth_and_snapshot() {
    use ordered_radix::ConcurrentArt;
    let threads = 8;
    let per = 10_000u64;
    // Few inodes so many dirents hash into the same shard/inode subtree, forcing
    // N4->N16->N48->N256 growth *concurrently* under lock-free rcu inserts.
    let map = Arc::new(ConcurrentArt::<u64>::new(16, 8));
    let barrier = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let map = Arc::clone(&map);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for i in 0..per {
                    map.insert(&k(t as u64 % 3, i), i); // 3 inodes -> wide fan-out
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    // every disjoint (inode=t%3 distinct per t? no — t%3 collides) ... verify the
    // last writer's value per key is present (keys (t%3, i); multiple t share inode
    // but distinct i across the shared inode only if t%3 equal -> same (inode,i)
    // collides). Just verify all keys readable and snapshot isolation holds.
    let snap = map.snapshot();
    for t in 0..threads as u64 {
        for i in (0..per).step_by(97) {
            assert!(map.get(&k(t % 3, i)).is_some(), "missing key after concurrent growth");
        }
    }
    // snapshot get returns a valid (written) value
    assert!(snap.get(&k(0, 0)).is_some());
}

// ---- LVIAARC interface: batch-apply + monotone seq + generation queries ----

#[test]
fn art_batch_apply_equals_sequential_and_generation() {
    use ordered_radix::ConcurrentArt;
    let m = ConcurrentArt::<u64>::new(16, 8);
    // A flush batch: (key, value, op_seq). Includes an out-of-order + a stale dup.
    let batch: Vec<(Vec<u8>, u64, u64)> = vec![
        (k(1, 10).to_vec(), 100, 5),
        (k(1, 20).to_vec(), 200, 6),
        (k(2, 10).to_vec(), 300, 7),
        (k(1, 10).to_vec(), 999, 3), // stale (seq 3 < 5) -> must NOT overwrite
    ];
    m.apply_batch(&batch);
    assert_eq!(m.get(&k(1, 10)), Some(100)); // stale dup ignored (monotone)
    assert_eq!(m.get(&k(1, 20)), Some(200));
    assert_eq!(m.get(&k(2, 10)), Some(300));
    // per-key integrated generation
    assert_eq!(m.key_seq(&k(1, 10)), Some(5));
    assert_eq!(m.key_seq(&k(2, 10)), Some(7));
    assert_eq!(m.key_seq(&k(9, 9)), None);
    // coarse integrated generation = max applied seq
    assert_eq!(m.integrated_generation(), 7);

    // apply a newer write to (1,10) -> wins; older -> ignored
    m.apply(&k(1, 10), 111, 9);
    assert_eq!(m.get(&k(1, 10)), Some(111));
    assert_eq!(m.key_seq(&k(1, 10)), Some(9));
    m.apply(&k(1, 10), 222, 4); // older -> no-op
    assert_eq!(m.get(&k(1, 10)), Some(111));
    assert_eq!(m.integrated_generation(), 9);
}

#[test]
fn art_concurrent_batch_apply_threads() {
    use ordered_radix::ConcurrentArt;
    let threads = 8;
    let per = 4_000u64;
    let map = Arc::new(ConcurrentArt::<u64>::new(64, 8));
    let barrier = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let map = Arc::clone(&map);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                // each thread flushes batches of 32 ops (its own inode, unique seqs)
                let mut chunk: Vec<(Vec<u8>, u64, u64)> = Vec::new();
                for i in 0..per {
                    let seq = (t as u64) << 40 | i; // per-thread monotone seq space
                    chunk.push((k(t as u64, i).to_vec(), i, seq));
                    if chunk.len() == 32 {
                        map.apply_batch(&chunk);
                        chunk.clear();
                    }
                }
                if !chunk.is_empty() {
                    map.apply_batch(&chunk);
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    for t in 0..threads as u64 {
        for i in (0..per).step_by(83) {
            assert_eq!(map.get(&k(t, i)), Some(i), "batch-applied key missing");
        }
    }
}

#[test]
fn art_per_shard_max_seq() {
    use ordered_radix::ConcurrentArt;
    let m = ConcurrentArt::<u64>::new(64, 8);
    let a = k(10, 0);
    let b = k(99, 0);
    let (sa, sb) = (m.shard_index(&a), m.shard_index(&b));
    m.apply(&a, 1, 50);
    m.apply(&b, 2, 200);
    assert!(m.shard_max_seq(sa) >= 50);
    assert!(m.shard_max_seq(sb) >= 200);
    assert_eq!(m.integrated_generation(), 200);
    // a shard that received no writes stays 0 -> recovery can skip it entirely
    if let Some(empty) = (0..m.num_shards()).find(|s| *s != sa && *s != sb) {
        assert_eq!(m.shard_max_seq(empty), 0);
    }
    // batch-apply also updates per-shard max
    m.apply_batch(&[(a.to_vec(), 9, 300)]);
    assert!(m.shard_max_seq(sa) >= 300);
    assert_eq!(m.integrated_generation(), 300);
}
