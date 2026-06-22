//! Concurrent throughput of `ConcurrentArt` (lock-free ART): insert scaling,
//! wait-free read scaling, and hot-shard contention.
//! Run: `cargo run --release --features concurrent --example radix_cbench -- [max_threads]`.

#[cfg(not(feature = "concurrent"))]
fn main() {
    eprintln!("rebuild with --features concurrent");
}

#[cfg(feature = "concurrent")]
fn main() {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::time::Instant;

    use portable_maps_and_sets::radix::ConcurrentArt;

    let max_t: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(8);
    let per: u64 = 400_000; // ops per thread

    fn key(inode: u64, off: u64) -> [u8; 18] {
        let mut k = [0u8; 18];
        k[0..8].copy_from_slice(&inode.to_be_bytes());
        k[8..16].copy_from_slice(&off.to_be_bytes());
        k
    }

    println!("== ConcurrentArt throughput (lock-free ART) — {per} ops/thread ==\n");

    // ---- (1) insert scaling: disjoint inodes per thread (shards spread) ----
    println!("(1) insert scaling — disjoint inodes (different shards)");
    println!("    threads   M ins/s   scaling");
    let mut base = 0.0;
    for &t in [1usize, 2, 4, 8].iter().filter(|&&t| t <= max_t) {
        let map = Arc::new(ConcurrentArt::<u64>::new(256, 8));
        let barrier = Arc::new(Barrier::new(t));
        let start = Instant::now();
        let handles: Vec<_> = (0..t)
            .map(|tid| {
                let map = Arc::clone(&map);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    for i in 0..per {
                        map.insert(&key(tid as u64 * 1_000_000 + (i >> 6), i), i); // spread inodes
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let mops = (t as u64 * per) as f64 / start.elapsed().as_secs_f64() / 1e6;
        if t == 1 {
            base = mops;
        }
        println!("    {t:>7}   {mops:>7.2}   {:>5.2}x", mops / base);
    }

    // ---- (2) read scaling: wait-free reads over a populated map ----
    println!("\n(2) wait-free read scaling — over a 1.6M-key map");
    let map = Arc::new(ConcurrentArt::<u64>::new(256, 8));
    for inode in 0..25_000u64 {
        for off in 0..64u64 {
            map.insert(&key(inode, off), off);
        }
    }
    println!("    threads   M get/s   scaling");
    let mut rbase = 0.0;
    for &t in [1usize, 2, 4, 8].iter().filter(|&&t| t <= max_t) {
        let barrier = Arc::new(Barrier::new(t));
        let hits = Arc::new(AtomicU64::new(0));
        let start = Instant::now();
        let handles: Vec<_> = (0..t)
            .map(|tid| {
                let map = Arc::clone(&map);
                let barrier = Arc::clone(&barrier);
                let hits = Arc::clone(&hits);
                thread::spawn(move || {
                    barrier.wait();
                    let mut x = 0x9E37u64.wrapping_mul(tid as u64 + 1);
                    let mut local = 0u64;
                    for _ in 0..per {
                        x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
                        if map.get(&key((x >> 20) % 25_000, x % 64)).is_some() {
                            local += 1;
                        }
                    }
                    hits.fetch_add(local, Ordering::Relaxed);
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let mops = (t as u64 * per) as f64 / start.elapsed().as_secs_f64() / 1e6;
        if t == 1 {
            rbase = mops;
        }
        println!("    {t:>7}   {mops:>7.2}   {:>5.2}x", mops / rbase);
        std::hint::black_box(hits.load(Ordering::Relaxed));
    }

    // ---- (3) hot-shard contention: all threads write the SAME inode ----
    if max_t >= 2 {
        println!("\n(3) hot-shard contention — all {} threads insert into ONE inode (same shard)", max_t.min(8));
        let t = max_t.min(8);
        let map = Arc::new(ConcurrentArt::<u64>::new(256, 8));
        let barrier = Arc::new(Barrier::new(t));
        let start = Instant::now();
        let handles: Vec<_> = (0..t)
            .map(|tid| {
                let map = Arc::clone(&map);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    for i in 0..per {
                        map.insert(&key(0, tid as u64 * per + i), i); // same inode 0 -> same shard
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let mops = (t as u64 * per) as f64 / start.elapsed().as_secs_f64() / 1e6;
        println!("    {t} threads, one shard: {mops:.2} M ins/s  (vs disjoint above — shows the lock-free CoW retry tax under same-shard contention)");
    }

    println!("\n(reads are wait-free → should scale ~linearly; disjoint inserts scale until shard/alloc contention; one-shard inserts serialize on the root CAS = the CoW lock-free retry cost.)");
}
