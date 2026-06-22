//! Step-level concurrency *model* (no real threads / atomics — `unsafe`-free).
//!
//! We model each operation as a state machine that advances by one bounded
//! "step" per scheduler turn. An adversarial scheduler interleaves operations
//! turn-by-turn. This lets us *validate the concurrency claims as model
//! properties* without unsafe atomics:
//!
//! * **Wait-free writes:** every writer completes within a CONSTANT number of
//!   its own turns (`KEY_LEN + 1`), regardless of how many other writers run or
//!   how adversarially they interleave. There is no retry loop, because radix
//!   has no structural rebalancing — the publish is a single step.
//! * **The throughput tax the user softly allowed:** a `LockFreeRetry` writer
//!   (optimistic CAS-retry, the usual lock-free style) is *cheaper with no
//!   contention* but, under an adversary, can be made to retry without bound —
//!   an unbounded tail. The wait-free writer pays a small, fixed extra cost
//!   always, and in exchange its worst case is BOUNDED. We measure both.
//! * **Linearizability:** writers publish at a single step (their linearization
//!   point); replaying writers in publish-order reproduces the final state.

use std::collections::BTreeMap;

use crate::key::KEY_LEN;
use crate::store::Value;
use crate::trie::RadixTrie;

/// Tiny deterministic PRNG (splitmix64) — keeps the crate dependency-free.
pub struct Rng(pub u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next_u64() % n
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Strategy {
    /// Bounded steps, single-step publish, no retry. Wait-free.
    WaitFree,
    /// Optimistic read-validate-publish; retries on conflict. Lock-free, not
    /// wait-free (unbounded retries under an adversary).
    LockFreeRetry,
}

/// Shared state the scheduler mutates one step at a time.
pub struct Shared {
    pub trie: RadixTrie<Value>,
    /// Per-key write counter (the "version" a LockFreeRetry writer validates).
    pub ver: BTreeMap<[u8; KEY_LEN], u64>,
    pub tick: u64,
}

impl Shared {
    pub fn new() -> Self {
        Shared {
            trie: RadixTrie::new(),
            ver: BTreeMap::new(),
            tick: 0,
        }
    }
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

const PUB_PREP: u32 = KEY_LEN as u32; // descend done; about to publish
const PUB_VALIDATE: u32 = KEY_LEN as u32 + 1;

pub struct Writer {
    pub strategy: Strategy,
    pub key: [u8; KEY_LEN],
    pub value: Value,
    phase: u32,
    observed: u64,
    pub turns: u32,
    pub done: bool,
    pub lin_tick: Option<u64>,
}

impl Writer {
    pub fn new(strategy: Strategy, key: [u8; KEY_LEN], value: Value) -> Self {
        Writer {
            strategy,
            key,
            value,
            phase: 0,
            observed: 0,
            turns: 0,
            done: false,
            lin_tick: None,
        }
    }

    /// Advance one bounded step against the shared state.
    pub fn step(&mut self, s: &mut Shared) {
        if self.done {
            return;
        }
        self.turns += 1;
        s.tick += 1;
        match self.strategy {
            Strategy::WaitFree => {
                if self.phase < PUB_PREP {
                    self.phase += 1; // bounded descend / ensure-node
                } else {
                    // Single-step publish == linearization point. No retry.
                    self.trie_publish(s);
                }
            }
            Strategy::LockFreeRetry => {
                if self.phase < PUB_PREP {
                    self.phase += 1;
                } else if self.phase == PUB_PREP {
                    self.observed = *s.ver.get(&self.key).unwrap_or(&0); // read version
                    self.phase = PUB_VALIDATE;
                } else {
                    let cur = *s.ver.get(&self.key).unwrap_or(&0);
                    if cur == self.observed {
                        self.trie_publish(s); // CAS succeeded
                    } else {
                        self.phase = PUB_PREP; // conflict -> retry (unbounded!)
                    }
                }
            }
        }
    }

    fn trie_publish(&mut self, s: &mut Shared) {
        s.trie.insert(&self.key, self.value.clone());
        *s.ver.entry(self.key).or_insert(0) += 1;
        self.lin_tick = Some(s.tick);
        self.done = true;
    }
}

/// Run a batch of writers under a random adversarial interleaving until all
/// complete. Returns the per-writer turn counts (in input order).
pub fn run_interleaved(shared: &mut Shared, writers: &mut [Writer], rng: &mut Rng) {
    let mut remaining: Vec<usize> = (0..writers.len()).collect();
    while !remaining.is_empty() {
        let pick = rng.below(remaining.len() as u64) as usize;
        let wi = remaining[pick];
        writers[wi].step(shared);
        if writers[wi].done {
            remaining.swap_remove(pick);
        }
    }
}

/// Result of the contention contrast.
#[derive(Debug, Clone, Copy)]
pub struct ContentionResult {
    pub strategy: Strategy,
    pub spoilers: u32,
    /// Turns the *victim* needed to commit one write under maximal adversarial
    /// interference from `spoilers` competing same-key writers.
    pub victim_turns: u32,
    /// Whether the victim stayed within the wait-free bound (KEY_LEN + 1).
    pub within_bound: bool,
}

/// Adversarial same-key contention: one victim of `strategy` tries to commit a
/// write while `spoilers` competing writers repeatedly publish to the SAME key,
/// each one slipped in just before the victim would publish. Demonstrates that a
/// LockFreeRetry victim is starved (turns grow with `spoilers`) while a WaitFree
/// victim is unaffected (constant turns).
pub fn contention_demo(strategy: Strategy, spoilers: u32) -> ContentionResult {
    let mut s = Shared::new();
    let key = crate::key::encode(7, 7, 1);
    let mut victim = Writer::new(strategy, key, Value::Inode(1));
    let safety_cap = (KEY_LEN as u32 + 2) * (spoilers + 4) + 100; // avoid an infinite loop

    let mut spoiled = 0u32;
    while !victim.done && victim.turns < safety_cap {
        // Drive the victim up to the brink of publishing.
        let at_brink = match strategy {
            Strategy::WaitFree => victim.phase >= PUB_PREP,
            Strategy::LockFreeRetry => victim.phase == PUB_VALIDATE,
        };
        if at_brink && spoiled < spoilers {
            // Adversary slips a competing same-key write to completion first.
            let mut spoiler = Writer::new(Strategy::WaitFree, key, Value::Inode(100 + spoiled as u64));
            while !spoiler.done {
                spoiler.step(&mut s);
            }
            spoiled += 1;
        }
        victim.step(&mut s);
    }

    ContentionResult {
        strategy,
        spoilers,
        victim_turns: victim.turns,
        within_bound: victim.turns <= KEY_LEN as u32 + 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::encode;

    #[test]
    fn waitfree_writer_is_bounded() {
        let mut s = Shared::new();
        let mut w = Writer::new(Strategy::WaitFree, encode(1, 2, 1), Value::Inode(9));
        while !w.done {
            w.step(&mut s);
        }
        assert_eq!(w.turns, KEY_LEN as u32 + 1);
        assert_eq!(s.trie.get(&encode(1, 2, 1)), Some(&Value::Inode(9)));
    }

    #[test]
    fn waitfree_bounded_under_contention_lockfree_is_not() {
        let wf = contention_demo(Strategy::WaitFree, 32);
        let lf = contention_demo(Strategy::LockFreeRetry, 32);
        // Wait-free victim is unaffected by 32 spoilers.
        assert!(wf.within_bound, "wait-free should stay bounded: {:?}", wf);
        assert_eq!(wf.victim_turns, KEY_LEN as u32 + 1);
        // Lock-free-retry victim is starved: turns grow with spoilers.
        assert!(!lf.within_bound, "lock-free-retry should blow past the bound: {:?}", lf);
        assert!(lf.victim_turns > wf.victim_turns);
    }

    #[test]
    fn interleaved_writes_are_linearizable() {
        let mut s = Shared::new();
        let mut rng = Rng::new(0xDEAD_BEEF);
        // Several writers, some to the SAME key (last publish wins).
        let mut writers = vec![
            Writer::new(Strategy::WaitFree, encode(1, 0, 1), Value::Inode(10)),
            Writer::new(Strategy::WaitFree, encode(1, 0, 1), Value::Inode(11)),
            Writer::new(Strategy::WaitFree, encode(1, 1, 1), Value::Inode(20)),
            Writer::new(Strategy::WaitFree, encode(2, 0, 1), Value::Inode(30)),
        ];
        run_interleaved(&mut s, &mut writers, &mut rng);

        // Oracle: apply writers in publish (lin_tick) order; last write per key wins.
        let mut order: Vec<&Writer> = writers.iter().collect();
        order.sort_by_key(|w| w.lin_tick.unwrap());
        let mut oracle: BTreeMap<[u8; KEY_LEN], Value> = BTreeMap::new();
        for w in order {
            oracle.insert(w.key, w.value.clone());
        }
        for (k, v) in &oracle {
            assert_eq!(s.trie.get(k), Some(v), "state must match the linearization");
        }
        // Every writer was bounded.
        assert!(writers.iter().all(|w| w.turns == KEY_LEN as u32 + 1));
    }
}
