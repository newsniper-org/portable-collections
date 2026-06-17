# Queue-family data structures — research library

Papers from **2024 onward** on **queues, stacks, deques, and priority queues**,
gathered for the `portable-queues` crate of `portable-collections`.

Downloaded 2026-06-17 from arXiv (all freely available; the arXiv id is the
filename suffix). Each entry: filename — title (authors/venue where known) —
one-line summary.

**Scope note.** Most recent work in this space is *concurrent / lock-free* (CAS,
helper mechanisms, manual memory reclamation) — relevant for a future concurrent
queue but in tension with this workspace's `unsafe`-free + `no_std` policy, which
a safe (single-threaded, index-arena, or functional) port would have to respect.
The persistent and purely-functional entries are the most directly portable.

---

## A. Concurrent / lock-free FIFO queues

- `no-cords-attached-coordination-free-lock-free-queues_2511.09410.pdf` —
  *No Cords Attached: Coordination-Free Concurrent Lock-Free Queues* (2025).
  Cyclic Memory Protection (CMP): enqueue, dequeue, and reclamation all proceed
  lock-free and uncoordinated; each node gets a cycle timestamp and is reclaimed
  only when both CLAIMED and outside a sliding protection window. Attacks the
  throughput-vs-FIFO-vs-memory-safety trilemma.
- `corec-nonblocking-single-queue-receive-driver_2401.12815.pdf` —
  *COREC: Concurrent Non-Blocking Single-Queue Receive Driver for Low Latency*
  (2024). A systems-level lock-free queue for the network receive path.
- `fair-kernel-lock-free-claim-release-protocol_2510.10818.pdf` —
  *A Fair Kernel-Lock-Free Claim/Release Protocol for Shared Object Access in
  Cooperatively Scheduled Runtimes* (2025). A lock-free `waitQueue` (multiple
  concurrent enqueue/dequeue) underpins a fair claim/release protocol.
  (Boundary: a protocol more than a standalone data structure.)

## B. Relaxed FIFO queues — trade strict ordering for throughput

- `relaxation-for-efficient-asynchronous-queues_2503.02164.pdf` —
  *Relaxation for Efficient Asynchronous Queues* (2025). Weakening the queue's
  ordering guarantees lets most dequeue instances return after only local
  computation → a low amortized cost per operation.
- `blockfifo-multififo-scalable-relaxed-queues_2507.22764.pdf` —
  *BlockFIFO & MultiFIFO: Scalable Relaxed Queues* (Koch & Sanders, PPoPP '25).
  BlockFIFO is a bounded, lock-free, relaxed FIFO over a fixed-size ring buffer;
  MultiFIFO replaces sequential priority queues with in-place ring buffers tagged
  by insertion timestamp.

## C. Concurrent priority queues

- `engineering-multiqueues-relaxed-concurrent-priority-queues_2504.11652.pdf` —
  *Engineering MultiQueues: Fast Relaxed Concurrent Priority Queues*
  (Sanders et al., 2025). A relaxed PQ built from several sequential PQs, scaled
  by element buffering, batched internal operations, and cache-locality tuning;
  outperforms prior relaxed designs.
- `smartpq-numa-adaptive-concurrent-priority-queue_2406.06900.pdf` —
  *SmartPQ: An Adaptive Concurrent Priority Queue for NUMA Architectures* (2024).
  Self-tunes between NUMA-oblivious and NUMA-aware modes; ~1.87× over SprayList.
- `pipq-insert-optimized-concurrent-priority-queue_2508.16023.pdf` —
  *PIPQ: Strict Insert-Optimized Concurrent Priority Queue* (2025). Two levels: a
  per-thread worker level for fast parallel inserts + a leader level holding the
  top elements for delete-min. Keeps **strict** (not relaxed) priority order.
- `concurrent-double-ended-priority-queues_2508.13399.pdf` —
  *Concurrent Double-Ended Priority Queues* (2025). A general transformation
  turning any concurrent single-ended PQ into a linearizable double-ended PQ.

## D. Deques (double-ended queues)

- `verified-functional-catenable-real-time-deques_2505.07681.pdf` —
  *Verified Purely Functional Catenable Real-Time Deques* (2025). First — and
  first *verified* — implementation of Kaplan–Tarjan catenable deques:
  push/pop + inject/eject + concatenation, persistent (immutable), O(1)
  worst-case per operation; in pure-functional OCaml and in Rocq/Gallina with
  machine-checked proofs. **The most `unsafe`-free / portable item here.**
- `adaptive-asynchronous-work-stealing-deque_2401.04494.pdf` —
  *Adaptive Asynchronous Work-Stealing for Distributed Load-Balancing in
  Heterogeneous Systems* (2024). Work-stealing scheduling over per-worker deques.
  (Boundary: a scheduling algorithm; the deque is its substrate.)

## E. Concurrent stacks

- `sharded-elimination-combining-concurrent-stacks_2601.04523.pdf` —
  *Sharded Elimination and Combining for Highly-Efficient Concurrent Stacks*
  (PPoPP '26). Combines sharding with the classic elimination-array + combining
  techniques for a highly scalable lock-free stack.

## F. Persistent (non-volatile-memory) queues

- `highly-efficient-persistent-fifo-queues_2402.17674.pdf` —
  *Highly-Efficient Persistent FIFO Queues* (2024). A durable (NVM) FIFO queue
  that issues only one pair of persistence instructions per enqueue/dequeue.

## G. GPU / specialized

- `multi-level-multi-queue-sssp-gpu_2602.10080.pdf` —
  *Beyond a Single Queue: Multi-Level-Multi-Queue as an Effective Design for
  SSSP Problems on GPUs* (2026). A multi-level, priority-queue-like frontier
  structure tuned to the GPU memory hierarchy for shortest-path search.

---

## Relevance to `portable-queues`

`portable-queues` targets `no_std`, `unsafe`-free, generic queue / stack / deque /
priority-queue types. Filtering these papers by that policy:

- **Directly portable (safe; sequential or functional).** The catenable
  real-time deque (D) is exactly the kind of structure a safe `no_std` crate can
  host — purely functional, persistent, O(1) worst-case. The *relaxed-ordering
  ideas* (B, and MultiQueue in C) can inform a single-threaded **batched** PQ even
  without concurrency.
- **Concurrent core; needs a safe re-expression.** The lock-free queues / stacks
  / PQs (A, C, E) are built on CAS + manual reclamation. A safe port would lean
  on an index arena or `crossbeam`-style epoch reclamation behind a feature,
  never raw atomics + `unsafe` — the same move the B-tree library made for ART.
  The *designs* (MultiQueue's multi-PQ relaxation, elimination/combining for
  stacks, the single→double-ended PQ transformation) are the transferable part.
- **Out of current scope.** NVM persistence (F) and GPU memory tiering (G) — the
  external-memory / hardware bucket the data-structures library also set aside.

### Most promising first targets

1. A **safe, sequential, generic priority queue** (binary heap / d-ary heap) with
   the workspace's `ScopedRollback` contract — the baseline every fancier PQ must
   beat, mirroring how `FlatRadixBimap` anchored the bimap line.
2. A **persistent / functional deque** along the lines of D — `no_std`-friendly
   and `unsafe`-free by construction.
3. Relaxed/batched PQ ideas (B, C) as a later concurrency-or-throughput experiment
   behind a feature, once a sequential baseline exists.
