//! Loom model-check of the wait-free write **gate + combine core**.
//!
//! `waitfree.rs` itself can't be loom-checked (arc-swap uses its own atomics,
//! not loom's instrumented ones). So this is a faithful re-expression of the
//! protocol core in loom atomics — single shard, single contended key, N writers
//! with distinct seqs `1..=N` — over which loom EXHAUSTIVELY explores all
//! interleavings (under the C11 memory model) and checks:
//!
//!   * SAFETY — the resident value is always the highest seq (N). This is the
//!     monotone seq-stamped apply (FIX 2): a stale lower-seq write can never
//!     overwrite the max, i.e. NO lost update / double-fold resurrection.
//!   * COMPLETION — every writer finishes (loom flags any deadlock; the
//!     bounded-rounds assert below turns any livelock into a loud failure).
//!   * BOUNDED ROUNDS — a slow-path writer commits within `2N+5` help rounds in
//!     EVERY interleaving (the wait-free witness, machine-checked for small N).
//!
//! What loom does and does not give: it is an exhaustive proof of the above for
//! the checked thread counts (N = 2, 3). The asymptotic O(K+P) starvation-free
//! bound (gate => only slot-scanning combines win while a desc is announced)
//! remains the analytical argument; loom's small-instance results corroborate it
//! and would catch any concrete safety/ordering bug in the gate+combine logic.
//!
//! Run:  RUSTFLAGS="--cfg loom" cargo test --release --test loom_waitfree
//! (optionally LOOM_MAX_PREEMPTIONS=3 to bound exploration depth)
#![cfg(loom)]

use loom::sync::atomic::{AtomicBool, AtomicUsize, Ordering::*};
use loom::sync::Arc;

struct State {
    /// Resident value for the single key = the max applied seq (0 = empty).
    root: AtomicUsize,
    /// announce[t] = the seq thread t is committing (0 = not announced).
    announce: Vec<AtomicUsize>,
    /// done[t] = thread t's announced write has been applied by some combine.
    done: Vec<AtomicBool>,
    /// outstanding announcements; gates the fast path (FIX 1).
    pending: AtomicUsize,
}

/// One combine round: fold every announced, not-done writer into the new max and
/// publish with one CAS; on success mark them all done.
fn help(s: &State, n: usize) {
    let cur = s.root.load(Acquire);
    let mut maxseq = cur;
    let mut batch = vec![false; n];
    let mut any = false;
    for t in 0..n {
        let a = s.announce[t].load(Acquire);
        if a != 0 && !s.done[t].load(Acquire) {
            batch[t] = true;
            any = true;
            if a > maxseq {
                maxseq = a;
            }
        }
    }
    if !any {
        return;
    }
    if s.root.compare_exchange(cur, maxseq, AcqRel, Acquire).is_ok() {
        for (t, &b) in batch.iter().enumerate() {
            if b {
                s.done[t].store(true, Release);
            }
        }
    }
}

/// The wait-free write of thread `t` (seq = t+1). `k` = fast-path attempts.
fn put(s: &State, t: usize, n: usize, k: usize, cap: usize) {
    let seq = t + 1;

    // FAST PATH — gated by `pending` (FIX 1).
    let mut attempt = 0;
    while attempt < k && s.pending.load(Acquire) == 0 {
        attempt += 1;
        let cur = s.root.load(Acquire);
        if cur >= seq {
            return; // superseded by a newer write -> done
        }
        if s.root.compare_exchange(cur, seq, AcqRel, Acquire).is_ok() {
            return;
        }
    }

    // SLOW PATH — announce, then help-combine until done.
    s.announce[t].store(seq, Release);
    s.pending.fetch_add(1, AcqRel);
    let mut rounds = 0;
    loop {
        help(s, n);
        rounds += 1;
        if s.done[t].load(Acquire) {
            break;
        }
        assert!(rounds <= cap, "wait-free bound exceeded: {rounds} rounds (n={n})");
    }
    s.announce[t].store(0, Release);
    s.pending.fetch_sub(1, AcqRel);
}

fn model(n: usize, k: usize) {
    loom::model(move || {
        let st = Arc::new(State {
            root: AtomicUsize::new(0),
            announce: (0..n).map(|_| AtomicUsize::new(0)).collect(),
            done: (0..n).map(|_| AtomicBool::new(false)).collect(),
            pending: AtomicUsize::new(0),
        });
        let cap = 2 * n + 5;
        let handles: Vec<_> = (0..n)
            .map(|t| {
                let st = st.clone();
                loom::thread::spawn(move || put(&st, t, n, k, cap))
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        // SAFETY: monotone apply must leave the maximum seq resident.
        assert_eq!(st.root.load(Acquire), n, "max-seq write must win (no lost update)");
    });
}

#[test]
fn loom_pure_combine_2() {
    model(2, 0); // k=0: force every writer through announce + combine
}

#[test]
fn loom_mixed_fast_slow_2() {
    model(2, 1); // k=1: exercise the gated fast-path <-> combine handoff
}

#[test]
fn loom_pure_combine_3() {
    model(3, 0);
}
