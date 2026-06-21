//! Simulator: drives the prototype with a randomized FS-ish workload and
//! validates it three ways:
//!   1. **Differential correctness** — every read / range is cross-checked
//!      against an independent `BTreeMap`-backed oracle.
//!   2. **Crash consistency** — periodically replay a random journal prefix and
//!      verify the recovered state equals applying exactly that prefix.
//!   3. **Concurrency model** — wait-free write bound + linearizability +
//!      the wait-free-vs-lock-free contention contrast (the throughput tax).

use std::collections::BTreeMap;

use crate::conc::{contention_demo, run_interleaved, ContentionResult, Rng, Shared, Strategy, Writer};
use crate::journal::Op;
use crate::key::{decode, encode, Inode, Offset, SnapId, KEY_LEN};
use crate::snapshot::SnapshotTree;
use crate::store::{replay, resolve, FsCore, Value};

/// Independent reference store (whole-history `BTreeMap`) for differential tests.
struct Oracle {
    map: BTreeMap<[u8; KEY_LEN], Value>,
    snaps: SnapshotTree,
}

impl Oracle {
    fn new() -> Self {
        Oracle {
            map: BTreeMap::new(),
            snaps: SnapshotTree::new(),
        }
    }

    fn apply(&mut self, op: &Op) {
        match *op {
            Op::Put { inode, offset, snap, ref value } => {
                self.map.insert(encode(inode, offset, snap), value.clone());
            }
            Op::Snap { parent } => {
                self.snaps.add_child(parent);
            }
        }
    }

    fn versions(&self, inode: Inode, offset: Offset) -> Vec<(SnapId, Value)> {
        let lo = encode(inode, offset, 0);
        let hi = encode(inode, offset, u32::MAX);
        self.map
            .range(lo..=hi)
            .map(|(k, v)| {
                let (_, _, s) = decode(k);
                (s, v.clone())
            })
            .collect()
    }

    fn get(&self, inode: Inode, offset: Offset, read: SnapId) -> Option<Value> {
        resolve(&self.versions(inode, offset), read, &self.snaps)
    }

    fn range(&self, inode: Inode, lo_off: Offset, hi_off: Offset, read: SnapId) -> Vec<(Offset, Value)> {
        let lo = encode(inode, lo_off, 0);
        let hi = encode(inode, hi_off, u32::MAX);
        let mut by_off: BTreeMap<Offset, Vec<(SnapId, Value)>> = BTreeMap::new();
        for (k, v) in self.map.range(lo..=hi) {
            let (_, off, s) = decode(k);
            if off < hi_off {
                by_off.entry(off).or_default().push((s, v.clone()));
            }
        }
        by_off
            .into_iter()
            .filter_map(|(off, group)| resolve(&group, read, &self.snaps).map(|v| (off, v)))
            .collect()
    }
}

fn oracle_from_ops(ops: &[Op]) -> Oracle {
    let mut o = Oracle::new();
    for op in ops {
        o.apply(op);
    }
    o
}

#[derive(Debug, Default, Clone)]
pub struct Report {
    pub steps: u64,
    pub puts: u64,
    pub deletes: u64,
    pub snapshots: u64,
    pub reads: u64,
    pub ranges: u64,
    pub read_mismatches: u64,
    pub range_mismatches: u64,
    pub keys: usize,
    pub nodes: usize,
    pub snapshots_total: SnapId,
    pub max_exact_lookup_steps: u32,
    pub crash_checks: u64,
    pub crash_mismatches: u64,
    // concurrency
    pub wf_write_bound: u32,
    pub wf_max_turns_random: u32,
    pub lin_ok: bool,
    pub contention: Vec<ContentionResult>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.read_mismatches == 0
            && self.range_mismatches == 0
            && self.crash_mismatches == 0
            && self.lin_ok
            && self.contention.iter().all(|c| match c.strategy {
                Strategy::WaitFree => c.within_bound,
                Strategy::LockFreeRetry => true, // lock-free is *expected* to exceed
            })
            && self.max_exact_lookup_steps as usize <= KEY_LEN
    }
}

pub struct Config {
    pub seed: u64,
    pub steps: u64,
    pub inodes: u64,
    pub offsets: u64,
    pub crash_every: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            seed: 0x1234_5678_9ABC_DEF0,
            steps: 20_000,
            inodes: 64,
            offsets: 64,
            crash_every: 2_000,
        }
    }
}

pub fn run(cfg: &Config) -> Report {
    let mut rng = Rng::new(cfg.seed);
    let mut core = FsCore::new();
    let mut oracle = Oracle::new();
    let mut rep = Report {
        wf_write_bound: KEY_LEN as u32 + 1,
        ..Default::default()
    };

    let val = |rng: &mut Rng| -> Value {
        match rng.below(3) {
            0 => Value::Extent(rng.next_u64(), (rng.below(8) + 1) as u32),
            1 => Value::Dirent(rng.below(cfg.inodes)),
            _ => Value::Inode(rng.next_u64()),
        }
    };

    for step in 0..cfg.steps {
        rep.steps += 1;
        let snap_hi = core.snaps.count(); // valid snapshot ids are 1..=snap_hi
        let read_snap = 1 + rng.below(snap_hi as u64) as SnapId;

        match rng.below(100) {
            // 45% put
            n if n < 45 => {
                let inode = rng.below(cfg.inodes);
                let offset = rng.below(cfg.offsets);
                let v = val(&mut rng);
                core.put(inode, offset, read_snap, v.clone());
                oracle.apply(&Op::Put { inode, offset, snap: read_snap, value: v });
                rep.puts += 1;
            }
            // 10% delete
            n if n < 55 => {
                let inode = rng.below(cfg.inodes);
                let offset = rng.below(cfg.offsets);
                core.delete(inode, offset, read_snap);
                oracle.apply(&Op::Put { inode, offset, snap: read_snap, value: Value::Tombstone });
                rep.deletes += 1;
            }
            // 5% snapshot
            n if n < 60 => {
                let parent = 1 + rng.below(snap_hi as u64) as SnapId;
                let id_core = core.create_snapshot(parent);
                oracle.apply(&Op::Snap { parent });
                debug_assert_eq!(id_core, oracle.snaps.count(), "snapshot ids must stay in lockstep");
                rep.snapshots += 1;
            }
            // 25% point read (differential + bounded-step check)
            n if n < 85 => {
                let inode = rng.below(cfg.inodes);
                let offset = rng.below(cfg.offsets);
                let got = core.get(inode, offset, read_snap);
                let want = oracle.get(inode, offset, read_snap);
                if got != want {
                    rep.read_mismatches += 1;
                }
                // exact-key lookup to demonstrate bounded read steps
                let exact_snap = 1 + rng.below(snap_hi as u64) as SnapId;
                let (_, steps) = core.exact_lookup_steps(inode, offset, exact_snap);
                rep.max_exact_lookup_steps = rep.max_exact_lookup_steps.max(steps);
                rep.reads += 1;
            }
            // 15% range scan (differential)
            _ => {
                let inode = rng.below(cfg.inodes);
                let a = rng.below(cfg.offsets);
                let b = rng.below(cfg.offsets);
                let (lo, hi) = if a <= b { (a, b + 1) } else { (b, a + 1) };
                let got = core.range(inode, lo, hi, read_snap);
                let want = oracle.range(inode, lo, hi, read_snap);
                if got != want {
                    rep.range_mismatches += 1;
                }
                rep.ranges += 1;
            }
        }

        // Crash-consistency check: recover from a random journal prefix and
        // verify against an oracle built from exactly that prefix.
        if cfg.crash_every > 0 && step % cfg.crash_every == cfg.crash_every - 1 {
            rep.crash_checks += 1;
            let jlen = core.journal.len();
            let cut = if jlen == 0 { 0 } else { (rng.below(jlen as u64) + 1) as usize };
            let prefix = core.journal.prefix(cut);
            let recovered = replay(&prefix);
            let pref_oracle = oracle_from_ops(&prefix);
            // Sample keys/snapshots and compare recovered vs prefix-oracle.
            let mut bad = false;
            for _ in 0..256 {
                let inode = rng.below(cfg.inodes);
                let offset = rng.below(cfg.offsets);
                let scount = recovered.snaps.count();
                let rs = if scount == 0 { 1 } else { 1 + rng.below(scount as u64) as SnapId };
                if recovered.get(inode, offset, rs) != pref_oracle.get(inode, offset, rs) {
                    bad = true;
                    break;
                }
            }
            if bad {
                rep.crash_mismatches += 1;
            }
        }
    }

    rep.keys = core.trie_len();
    rep.nodes = core.trie_nodes();
    rep.snapshots_total = core.snaps.count();

    // ---- Concurrency model checks ----
    // (a) Random interleaving of many writers: every writer bounded + linearizable.
    {
        let mut s = Shared::new();
        let mut writers: Vec<Writer> = Vec::new();
        for i in 0..200u64 {
            // deliberately reuse some keys to create real conflicts
            let key = encode(rng.below(8), rng.below(8), 1);
            writers.push(Writer::new(Strategy::WaitFree, key, Value::Inode(i)));
        }
        run_interleaved(&mut s, &mut writers, &mut rng);
        rep.wf_max_turns_random = writers.iter().map(|w| w.turns).max().unwrap_or(0);

        // Linearizability: replay in publish order, last write per key wins.
        let mut order: Vec<&Writer> = writers.iter().collect();
        order.sort_by_key(|w| w.lin_tick.unwrap());
        let mut lin: BTreeMap<[u8; KEY_LEN], Value> = BTreeMap::new();
        for w in order {
            lin.insert(w.key, w.value.clone());
        }
        rep.lin_ok = lin.iter().all(|(k, v)| s.trie.get(k) == Some(v));
    }

    // (b) Contention contrast: wait-free vs lock-free-retry under N spoilers.
    for &spoilers in &[1u32, 4, 16, 64] {
        rep.contention.push(contention_demo(Strategy::WaitFree, spoilers));
        rep.contention.push(contention_demo(Strategy::LockFreeRetry, spoilers));
    }

    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulation_is_consistent() {
        let cfg = Config {
            seed: 99,
            steps: 8_000,
            inodes: 32,
            offsets: 32,
            crash_every: 1_000,
        };
        let rep = run(&cfg);
        assert_eq!(rep.read_mismatches, 0, "{:?}", rep);
        assert_eq!(rep.range_mismatches, 0, "{:?}", rep);
        assert_eq!(rep.crash_mismatches, 0, "{:?}", rep);
        assert!(rep.lin_ok);
        assert!(rep.max_exact_lookup_steps as usize <= KEY_LEN);
        assert!(rep.ok());
    }
}
