//! Run the simulator and print a report.
//!
//! Usage: `cargo run --release --bin simulate -- [steps] [seed]`

use radix_fs_prototype::conc::Strategy;
use radix_fs_prototype::sim::{run, Config};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut cfg = Config::default();
    if let Some(s) = args.get(1).and_then(|s| s.parse().ok()) {
        cfg.steps = s;
    }
    if let Some(s) = args.get(2).and_then(|s| s.parse().ok()) {
        cfg.seed = s;
    }

    println!("== radix-fs-prototype simulator ==");
    println!(
        "config: steps={} seed={:#x} inodes={} offsets={} crash_every={}\n",
        cfg.steps, cfg.seed, cfg.inodes, cfg.offsets, cfg.crash_every
    );

    let rep = run(&cfg);

    println!("workload:");
    println!("  ops={}  puts={} deletes={} snapshots={} reads={} ranges={}", rep.steps, rep.puts, rep.deletes, rep.snapshots, rep.reads, rep.ranges);
    println!("  snapshots_total={}", rep.snapshots_total);
    println!();
    println!("correctness (vs independent BTreeMap oracle):");
    println!("  read mismatches  : {}", rep.read_mismatches);
    println!("  range mismatches : {}", rep.range_mismatches);
    println!();
    println!("crash consistency (replay random journal prefix):");
    println!("  checks={} mismatches={}", rep.crash_checks, rep.crash_mismatches);
    println!();
    println!("structure (radix, no rebalancing):");
    println!("  live keys = {}", rep.keys);
    println!("  trie nodes = {} ({:.2} nodes/key)", rep.nodes, rep.nodes as f64 / rep.keys.max(1) as f64);
    println!("  max exact-lookup hops = {} (bound = KEY_LEN = {})  [bounded read]", rep.max_exact_lookup_steps, radix_fs_prototype::KEY_LEN);
    println!();
    println!("concurrency model:");
    println!("  wait-free write bound = {} turns/op", rep.wf_write_bound);
    println!("  random-interleave max writer turns = {}  (== bound: {})", rep.wf_max_turns_random, rep.wf_max_turns_random == rep.wf_write_bound);
    println!("  linearizable = {}", rep.lin_ok);
    println!();
    println!("  contention contrast (victim turns to commit 1 write vs N same-key spoilers):");
    println!("    {:>8}   {:>12}   {:>16}", "spoilers", "wait-free", "lock-free-retry");
    let mut spoilers_seen = std::collections::BTreeSet::new();
    for c in &rep.contention {
        spoilers_seen.insert(c.spoilers);
    }
    for sp in spoilers_seen {
        let wf = rep.contention.iter().find(|c| c.spoilers == sp && c.strategy == Strategy::WaitFree);
        let lf = rep.contention.iter().find(|c| c.spoilers == sp && c.strategy == Strategy::LockFreeRetry);
        let wfs = wf.map(|c| format!("{}{}", c.victim_turns, if c.within_bound { " (bounded)" } else { " (!)" })).unwrap_or_default();
        let lfs = lf.map(|c| format!("{}{}", c.victim_turns, if c.within_bound { "" } else { " (unbounded tail)" })).unwrap_or_default();
        println!("    {:>8}   {:>12}   {:>16}", sp, wfs, lfs);
    }
    println!();
    println!("  => wait-free trades a fixed per-op cost (the bound) for a BOUNDED worst case;");
    println!("     lock-free-retry is cheaper uncontended but its tail grows with contention.");
    println!();

    if rep.ok() {
        println!("RESULT: OK — all invariants held.");
    } else {
        eprintln!("RESULT: FAILED — see mismatches above.");
        std::process::exit(1);
    }
}
