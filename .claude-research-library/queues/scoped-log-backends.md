# ScopedLog backends — comparison & decision

A study of *additional* `ScopedLog<T>` implementations for `portable-queues`,
measured against the existing `VecLog` baseline. Generated 2026-06-17 from a
4-candidate analysis grounded in the queue research library.

## 1. Purpose & scope

`ScopedLog<T>` (in `portable-collection-primitives`, `: ScopedRollback + Push<T>
+ Pop<T>`) is an **undo ledger** — an SMT solver trail, a clause-DB per-scope
"what to drop on pop" list. Its load-bearing invariant (the five `ScopedRollback`
laws): *every item pushed after a mark survives until that mark is rolled back*,
and `drain_since` yields exactly that suffix LIFO so a caller can run per-item
cleanup. It exists to make the *primary-store ↔ undo-ledger desync* bug
unrepresentable.

## 2. The real workload

Unbounded growth; very frequent **infallible** `push`; occasional LIFO
`drain_since` on scope pop; small `Copy`-ish items (ids/tuples); marks held as
`usize` (`Checkpoint`) **never as a live `&T`**; single-threaded; strictly LIFO;
never branched or catenated.

## 3. Feature tiers

`bare no_std` (no alloc) · `alloc` · `std`. **`VecLog` is `ifstdoralloc!`-gated
(needs `alloc::Vec`), so the bare no_std tier currently has *zero* `ScopedLog`
impls.** That gap is the only thing the heap-tier baseline cannot fill itself.

## 4. Baseline: `VecLog`

`push` = amortized O(1) pointer-bump (infallible); `drain_since` = a single
`Vec::drain(from..).rev()` bulk move; `Mark = Checkpoint(usize)`; contiguous,
single allocation, reclaimed on pop, cache-friendly, offers `as_slice()`. It is
near-optimal for this workload.

## 5. Comparison

| Backend | `push` | `drain_since` | Memory | Tier | Beats `VecLog` when |
|---|---|---|---|---|---|
| **VecLog** *(baseline)* | amortized O(1) bump | one `Vec::drain.rev()` | unbounded, contiguous, reclaimed on pop | alloc | — (wins on every heap tier) |
| ChunkedLog | O(1), worse constant | O(k) per-elem + spine | unbounded, more overhead, no `as_slice` | alloc | only with a live `&T` held across `push` — **no caller** |
| PersistentLog | O(1) but alloc+refcount **per push** (~10–50×) | O(k) yield + O(1) revert | worse; pins a spine per open scope | alloc | only retain/branch/replay multiple versions — **never used** |
| HeaplessLog | O(1) **true** worst-case | O(k) `Option::take` | **bounded, static, no alloc** | **bare no_std** | only on a no-allocator target — **fills the one tier gap** |
| RingLog | O(1) but no sound full-policy | O(k) modular, only if no overwrite | bounded only nominally | bare (contract unhonorable) | **never** — category error |

## 6. Candidate verdicts

- **ChunkedLog — skip.** Its one edge (pointer/reference stability across pushes)
  has no consumer: the trail holds `usize` marks, and the borrow checker forbids
  holding `&T` across a `push`. Adds no tier; strictly more overhead than `VecLog`.
- **PersistentLog — skip.** O(1) silent `rollback_to` + structural sharing serve
  branch/retain/time-travel use-cases the strictly-LIFO discard-on-pop trail never
  exercises, while regressing the hot `push` with a per-element alloc+refcount and
  pinning a historical spine per open scope. Still needs `alloc` (fills no gap),
  and `Rc` is `Clone` not `Copy` (fights `Mark: Copy`). The catenable-deque paper
  (`2505.07681` §1.2) itself concedes compelling applications are thin.
- **HeaplessLog — defer.** A fixed inline `[Option<T>; N] + len`; `Mark` stays
  `Checkpoint(usize)`. The **only** candidate that compiles in bare no_std, so the
  sole way to give that tier *any* `ScopedLog`. Build behind a `heapless` feature
  ONLY when a real no-allocator consumer with a known static bound appears.
- **RingLog — reject.** Overwrite-oldest silently drops un-popped trail entries =
  the exact desync bug the trait forbids; fail-on-full breaks infallible `push`;
  wrap-around breaks `Mark = Checkpoint(usize)` (round-trip identity + overshoot).

## 7. The infallible-`push` vs fixed-capacity tension

`Push::push` is infallible, so a fixed-capacity backend (HeaplessLog) can only
**panic on full** — re-creating the surprise-panic failure mode this workspace
was extracted to eliminate. The clean remedy is a fallible `try_push -> Result<(),
CapacityError>` (a `HeaplessLog` inherent method, or a trait extension) so a full
buffer is a recoverable error, never a panic. Non-overwriting; fail loudly.

## 8. Reference grounding

- `blockfifo-multififo-scalable-relaxed-queues_2507.22764.pdf` — the bounded ring
  is **concurrency-motivated**, and its overwrite-oldest behavior is exactly the
  unsound move for an undo ledger.
- `verified-functional-catenable-real-time-deques_2505.07681.pdf` — the persistent
  backend; its own §1.2 concedes thin applications. Neither paper justifies
  beating `Vec` on this sequential trail.

## 9. Decision & roadmap

1. **Keep `VecLog` as the sole heap-tier `ScopedLog` impl.** It is correct and
   optimal; adding a heap backend would be net-negative (more code, worse
   constants, no consumer).
2. **`HeaplessLog` deferred** behind a `heapless` feature, pending a real
   bare-no_std consumer; ship with a fallible `try_push`.
3. **Ring buffers belong elsewhere** — as a separate honest bounded **FIFO/deque**
   type (a `Pull`-based queue with explicit `push`/`pull` and a documented
   full-policy), NOT under `ScopedLog`. This is where the just-added
   `Pull`/`Pop`/`Push` trait split pays off.

## 10. Open questions (for the user)

1. Any planned **bare-no_std / no-allocator** consumer of `ScopedLog` (firmware /
   MCU / pre-heap stage)? If not, `HeaplessLog` drops off the roadmap and `VecLog`
   is the final `ScopedLog` answer.
2. If a bare-tier consumer is real: extend the trait with a fallible `try_push`, or
   keep `push` infallible and add an inherent `try_push` on `HeaplessLog` only?
   (Decides whether a `CapacityError` lands in primitives.)
3. Plan a **bounded ring buffer as a separate `Pull`-based FIFO/deque** in
   `portable-queues` (independent of `ScopedLog`)? The ring concept is rejected as
   a `ScopedLog` but has a legitimate home as a queue — and you've already added
   `Pull`/`Pop` for exactly this split.
4. Any near-term need to RETAIN / BRANCH / replay multiple historical trail
   versions (parallel-portfolio solving, time-travel debugging)? Only that makes
   `PersistentLog` worth its cost.
