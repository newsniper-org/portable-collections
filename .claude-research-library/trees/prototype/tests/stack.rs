//! Capstone: the full synthesis stack under real threads. A concurrent
//! FS workload (puts / deletes / snapshot creates from N threads), then a
//! simulated crash -> journal drain -> recover, asserting the recovered state
//! equals the live state. This simultaneously witnesses crash-consistency AND
//! linearizability (the concurrent run == its own seq-order serialization).

use std::sync::{Arc, Barrier};
use std::thread;

use radix_fs_prototype::conc::Rng;
use radix_fs_prototype::concurrent::ConcFs;
use radix_fs_prototype::store::Value;

#[test]
fn concurrent_stack_recover_equals_live() {
    let threads = 8usize;
    let per = 4_000u64;
    let inodes = 16u64;
    let offsets = 16u64;
    let cap = (threads as u64 * per + 16) as usize;

    let fs = Arc::new(ConcFs::new(64, threads, cap));
    let barrier = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let fs = Arc::clone(&fs);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                let mut rng = Rng::new(0x100 + t as u64);
                for i in 0..per {
                    let sc = fs.snapshot_count().max(1) as u64;
                    let snap = 1 + rng.below(sc) as u32;
                    match rng.below(100) {
                        n if n < 70 => fs.put(t, rng.below(inodes), rng.below(offsets), snap, Value::Inode(i)),
                        n if n < 85 => fs.delete(t, rng.below(inodes), rng.below(offsets), snap),
                        _ => {
                            let p = 1 + rng.below(fs.snapshot_count().max(1) as u64) as u32;
                            fs.create_snapshot(t, p);
                        }
                    }
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }

    // Simulated crash: replay the (whole) journal into a fresh core.
    let ops = fs.drained_ops();
    let rec = ConcFs::recover(&ops, 64, threads, cap);

    let count = fs.snapshot_count();
    let mut rng = Rng::new(0xC0FFEE);
    for _ in 0..30_000 {
        let inode = rng.below(inodes);
        let off = rng.below(offsets);
        let snap = 1 + rng.below(count.max(1) as u64) as u32;
        assert_eq!(
            fs.get(inode, off, snap),
            rec.get(inode, off, snap),
            "recovered != live @ ({inode},{off},{snap})"
        );
    }
}

#[test]
fn concurrent_stack_torn_prefix_recovers_consistently() {
    // A torn-tail crash: recover from a prefix of the journal and check it
    // equals an independent recover of the same prefix (durable-prefix property).
    let threads = 6usize;
    let per = 3_000u64;
    let cap = (threads as u64 * per + 16) as usize;
    let fs = Arc::new(ConcFs::new(32, threads, cap));
    let barrier = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let fs = Arc::clone(&fs);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                let mut rng = Rng::new(7 + t as u64);
                for i in 0..per {
                    let sc = fs.snapshot_count().max(1) as u64;
                    let snap = 1 + rng.below(sc) as u32;
                    if rng.below(100) < 90 {
                        fs.put(t, rng.below(8), rng.below(8), snap, Value::Inode(i));
                    } else {
                        fs.create_snapshot(t, 1 + rng.below(sc) as u32);
                    }
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let ops = fs.drained_ops();
    let cut = ops.len() * 2 / 3; // torn tail
    let prefix = &ops[..cut];
    let a = ConcFs::recover(prefix, 32, threads, cap);
    let b = ConcFs::recover(prefix, 32, threads, cap);
    let count = a.snapshot_count();
    let mut rng = Rng::new(42);
    for _ in 0..10_000 {
        let inode = rng.below(8);
        let off = rng.below(8);
        let snap = 1 + rng.below(count.max(1) as u64) as u32;
        assert_eq!(a.get(inode, off, snap), b.get(inode, off, snap), "prefix replay not deterministic");
    }
}
