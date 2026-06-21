//! End-to-end demo of the full synthesis stack (`concurrent::ConcFs`):
//! wait-free radix map + in-key snapshots + journal durability, exercised by
//! real threads, then a simulated crash + recovery.
//!
//! Usage: cargo run --release --bin fsdemo -- [threads] [ops/thread]

use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

use radix_fs_prototype::conc::Rng;
use radix_fs_prototype::concurrent::ConcFs;
use radix_fs_prototype::store::Value;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let threads: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(8);
    let per: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(200_000);
    let inodes = 256u64;
    let offsets = 256u64;
    let cap = (threads as u64 * per + 16) as usize;

    println!("== full FS-core stack demo (wait-free map + snapshots + journal) ==");
    println!("threads={threads} ops/thread={per}\n");

    let fs = Arc::new(ConcFs::new(256, threads, cap));
    let barrier = Arc::new(Barrier::new(threads));
    let start = Instant::now();
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let fs = Arc::clone(&fs);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                let mut rng = Rng::new(0xABC + t as u64);
                let (mut puts, mut dels, mut snaps, mut reads) = (0u64, 0u64, 0u64, 0u64);
                for i in 0..per {
                    let sc = fs.snapshot_count().max(1) as u64;
                    let snap = 1 + rng.below(sc) as u32;
                    match rng.below(100) {
                        n if n < 55 => {
                            fs.put(t, rng.below(inodes), rng.below(offsets), snap, Value::Extent(i, 1));
                            puts += 1;
                        }
                        n if n < 65 => {
                            fs.delete(t, rng.below(inodes), rng.below(offsets), snap);
                            dels += 1;
                        }
                        n if n < 68 => {
                            fs.create_snapshot(t, 1 + rng.below(sc) as u32);
                            snaps += 1;
                        }
                        _ => {
                            let _ = fs.get(rng.below(inodes), rng.below(offsets), snap);
                            reads += 1;
                        }
                    }
                }
                (puts, dels, snaps, reads)
            })
        })
        .collect();
    let mut tot = (0u64, 0u64, 0u64, 0u64);
    for h in handles {
        let (p, d, s, r) = h.join().unwrap();
        tot.0 += p;
        tot.1 += d;
        tot.2 += s;
        tot.3 += r;
    }
    let dur = start.elapsed();
    let total_ops = threads as u64 * per;
    println!("workload ({:.3}s, {:.2} M ops/s):", dur.as_secs_f64(), total_ops as f64 / dur.as_secs_f64() / 1e6);
    println!("  puts={} deletes={} snapshot-creates={} reads={}", tot.0, tot.1, tot.2, tot.3);
    println!("  snapshots total={}", fs.snapshot_count());

    // ---- simulated crash + recovery ----
    println!("\n*** simulated crash: replaying the journal ***");
    let rstart = Instant::now();
    let ops = fs.drained_ops();
    let rec = ConcFs::recover(&ops, 256, threads, cap);
    println!("  journal records: {}; recovered in {:.3}s", ops.len(), rstart.elapsed().as_secs_f64());

    // ---- verify recovered == live ----
    let count = fs.snapshot_count();
    let mut rng = Rng::new(0xD00D);
    let mut checked = 0u64;
    let mut mismatch = 0u64;
    for _ in 0..200_000 {
        let inode = rng.below(inodes);
        let off = rng.below(offsets);
        let snap = 1 + rng.below(count.max(1) as u64) as u32;
        if fs.get(inode, off, snap) != rec.get(inode, off, snap) {
            mismatch += 1;
        }
        checked += 1;
    }
    println!("\nverification: {checked} sampled reads, {mismatch} recovered-vs-live mismatches");
    println!("  (recovered == live  =>  crash-consistent AND the concurrent run linearizes by op-seq)");

    if mismatch == 0 {
        println!("\nRESULT: OK — full stack consistent under real threads + crash recovery.");
    } else {
        eprintln!("\nRESULT: FAILED");
        std::process::exit(1);
    }
}
