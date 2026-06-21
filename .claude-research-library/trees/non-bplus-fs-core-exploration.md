# A non-B+tree FS core that is lock-free **and** soft-realtime — design-space exploration

Generated 2026-06-21. Question: is there a **non-B+tree** structure that matches/beats bcachefs as a filesystem core while *additionally* being genuinely **lock-free** and **soft-realtime (bounded worst-case)** — the two things bcachefs's OLC design does *not* guarantee?

Method: 6 non-B+tree candidate cores, each designed against the bcachefs bar and the four-way constraint, then adversarially stress-tested for the *simultaneous* satisfaction of all four (a design meeting any three but not the fourth FAILS). Grounded in the library.

## Convergent finding

**Every** candidate's stated advantage is *no global rebalancing*. The decisive structural fact: comparison B+trees rebalance via splits/merges (SMOs) that **propagate**, and making those lock-free forces **helping** (unbounded pathological work) — which breaks lock-freedom *and* bounded latency at once. Families with **no propagating SMOs** — radix/tries, log-structured/immutable, path-copying — are therefore the only place the four constraints can co-exist. With fixed-width FS keys (inode#||offset||snapshot), a radix trie has **constant depth** → genuinely O(1)-bounded, comparison-free, wait-free reads. That is the real prize, and it is strictly better than bcachefs's optimistic-lock-coupling on the read/progress/tail-latency axes.

**But no single off-the-shelf design satisfies all four simultaneously** — each fails at a specific, nameable intersection (below). The two survivors fail at *complementary* points, which is what makes a hybrid plausible.

## Scorecard

| Candidate | Lock-free | Soft-realtime (the crux) | FS feature set | vs bcachefs | Verdict |
|---|---|---|---|:--:|:--:|
| Lock-free Adaptive Radix Trie | partial | conditional | partial | matches | **viable-with-caveats** |
| Copy-on-write / path-copying persistent ra | yes (wait-free reads) | conditional → effectively NO once  | partial. Crash-consistency (atomic | below | **viable-with-caveats** |
| Lock-free / log-structured LSM | yes (wait-free reads) for immutabl | no (unbounded tail). TWO structura | partial. Crash-consistency NATIVE  | below | **unsuitable** |
| Lock-free cache-conscious skip list | partial. Point reads/lookups: genu | no (unbounded tail). FOUR distinct | partial. Snapshots: native and O(1 | matches | **unsuitable** |
| Lock-free hash/array-mapped trie with O(1) | yes (wait-free reads) for POINT LO | no (unbounded/amortized-only tail  | partial. Ordered range scans: REAL | below | **unsuitable** |
| Learned lock-free index | yes (lock-free), and wait-free rea | no (unbounded tail). FOUR unbounde | partial. Range scans: NATIVE and g | below | **unsuitable** |

## Per-candidate fatal flaw

### Lock-free Adaptive Radix Trie (ART/ROART) core — `viable-with-caveats`

- **Core:** Two-layer persistent radix index (PACTree shape). (1) Search layer = Adaptive Radix Trie (Node4/16/48/256 + path compression + lazy expansion) indexing partial keys — the "router", NO comparison rebalancing. (2) Data layer = doubly-linked list of slotted leaf nodes (B+tree-style, ~64 entries, finger
- **Best at:** No global rebalancing: the radix structure is insertion-order-independent and depth is CONSTANT for fixed-width FS keys, so the only structural ops are single-node resize (<=2 nodes) and LOCAL leaf split/merge — and those can be pushed off the critical path vi
- **Fatal flaw (4-way):** The (2)∧(4) collision is unresolved and is structural, not a tuning gap: durable linearizability and wait-free reads are mutually exclusive in THIS design. PACTree's own finding is that ROWEX non-blocking reads VIOLATE durable linearizability (a reader can observe a key whose store has not yet been persisted), which is why PACTree itself DROPPED non-blocking reads for optimistic persistent version LOCKS. The ART-2016 mechanism confirms why: ROWEX correctness rests on writers'
- **Bottom line:** Honest bottom line: this design wins the two axes radix is actually good at — bounded constant-hop comparison-free lookups (kills the split-cascade that forces B+tree/OLC into helping) and wait-free VOLATILE reads — and those are real, paper-grounded improvements over bcachefs on the progress and tail-latency axes. But it does NOT meet all four at once, and the failure is at the exact (2)∧(4) intersection the prompt flags. To make it actually work you must concede the headline: (a) accept optimistic persistent-vers

### Copy-on-write / path-copying persistent radix trie (snapshot-native) — `viable-with-caveats`

- **Core:** A fixed-depth radix trie keyed on the dense FS key (inode, offset, snapshot). The structure that replaces the B+tree is an IMMUTABLE/persistent trie where the only mutable cell is a single atomic root pointer. Each update path-copies the touched root-to-leaf radix path (the strict-shadowing of Rodeh
- **Best at:** No global rebalancing => no SMOs => the per-operation structural work is a fixed constant (O(trie-depth) path-copy), so the single thing that forces unbounded helping and unbounded worst-case latency in every B+tree (a split/merge cascading toward the root, wh
- **Fatal flaw (4-way):** Single global serialization at the root: every writer commits through ONE root CAS / flat-combiner, so write throughput is bounded by one core and cannot scale like bcachefs's per-subtree OLC concurrency — failing "perf >= bcachefs" (4). And the moment you add path compression to be cache-competitive, compressed-path splits become data-dependent structural mutations (refuting "no SMOs"), while the mandatory write-amp fix (BetrFS background healing) plus snapshot-subtree ref-c
- **Bottom line:** The read side is the real prize and it is genuinely won: wait-free, retry-free, bounded-depth reads with free snapshots and native range scans — strictly better than bcachefs's optimistic-lock-coupling on the read/snapshot progress axis, and crash-consistency via atomic root swap is cleaner than 48 recovery passes. That part is publishable and correct. What it would actually take to satisfy all four at once: (1) Solve the single-root bottleneck — a single global CAS/combiner cannot match bcachefs's per-subtree writ

### Lock-free / log-structured LSM (immutable sorted runs + lock-free memtable) — `unsuitable`

- **Core:** Replace the single B+tree with a versioned, partitioned LSM: one lock-free skiplist/hash memtable (the only mutable object) + a stack of IMMUTABLE sorted runs (SSTables) organized into levels, each level range-partitioned into fixed-size SSTables and grouped under FLSM "guards" (PebblesDB) so a key 
- **Best at:** Write-path dominance with NO global rebalancing: append-only writes + FLSM guards eliminate same-level rewrites, so (a) write amplification is 2.4-3x below leveling LSM and far below an in-place B+tree (flash-ideal), and (b) there are NO splits/merges/SMOs tha
- **Fatal flaw (4-way):** The mechanism that earns its lock-freedom is the same one that destroys its bounded latency. FLSM's "no same-level rewrite — append a fragment" is what makes per-guard compactions independent (→ sharded CAS publish, the lock-free win) AND what makes per-guard run count unbounded between compactions. Range scans cannot bloom-prune, so scan worst-case latency = overlapping-run count = how far compaction is behind = unbounded under adversarial write bursts. The only worst-case d
- **Bottom line:** It cleanly meets THREE of four — wait-free reads / lock-free publish (2), crash-consistency+snapshots+ordered scans (4 as a feature set), and write-amp dominance — but fails the crux (3) bounded worst-case latency, and that failure is structural, not an engineering gap. To rescue it you would have to cap per-guard run count with a HARD bound and run incremental/deterministic compaction that keeps the cap at all times (de-amortized FLSM) — but a hard run-count cap means compaction can no longer be deferred under a b

### Lock-free cache-conscious skip list (B-skiplist) — `unsuitable`

- **Core:** A blocked (cache-conscious) skip list — the B-skiplist of Luo et al. 2025 — used as the single unified store, the way bcachefs uses one B+tree for everything. Bottom level (level 0) is a sorted linked list of FAT NODES, each node ~1-4 cache lines (vs bcachefs's 128-256K nodes) holding B sorted (key,
- **Best at:** No global rebalancing + wait-free reads. Because a skip list has zero rotations and only local link/unlink + node-local overflow splits, the read path is a single lock-free/wait-free top-down descent that NEVER blocks behind a writer and NEVER triggers a root-
- **Fatal flaw (4-way):** Ordered range scans — the FS's core read (dir enumeration, extent maps) — cannot be done wait-free with bounded latency on a live lock-free linked structure. The only known fix (the Bw-tree's private buffered read-only node copies) abandons the wait-free-read on exactly the operation that justifies the design, and is bounded only per-node, not across node boundaries under concurrent splits. This sits precisely at the lock-free ∧ soft-realtime ∧ FS-feature triple-intersection 
- **Bottom line:** The design's own self-assessment is honest and largely correct — but it understates one flaw and that understatement is fatal. Point reads are genuinely wait-free and the tail-latency story for point-with-insert is real, so as a point-lookup interner-style store it is a strong contender. As an FS core it fails the four-way bar because ORDERED RANGE SCANS — not splits or reclamation — are the unfixable seam: a live lock-free linked ordered structure cannot give consistent wait-free bounded-latency iteration, and the

### Lock-free hash/array-mapped trie with O(1) snapshots (Ctrie-style) — `unsuitable`

- **Core:** A radix (MSB-first) trie of bounded depth d = ceil(W / r), where W = total key width (inode|offset|snapshot, e.g. 64+64+32 = ~160 bits) and r = radix stride per level (e.g. 4–8 bits). Replaces the B+tree entirely. Because branching is on key bits MSB-first (NOT a hash), in-order traversal yields sor
- **Best at:** NO global rebalancing. The fixed-depth radix structure has zero split/merge/SMO propagation, so (a) reads are genuinely wait-free with O(1)-WORST-CASE steps (Ko-proven), and (b) the maximum structural work for ANY update is a fixed constant d — eliminating the
- **Fatal flaw (4-way):** The four-way simultaneity fails because soft-realtime (leg 3) and performance (leg 4) both break while lock-freedom (leg 2) only half-holds. Decisive single flaw: there is NO per-operation worst-case bound on the write/snapshot side — Ko's updates are amortized O(c^2+log u) at the algorithm level (before reclamation), and the cited Wei-et-al snapshot READ is unbounded (proportional to CASes since snapshot), which is fatal precisely because FS snapshot-reads are a primary oper
- **Bottom line:** What would actually make it work narrows it so far it stops being this design. Keep ONLY: (i) the fixed-depth MSB-radix ordered trie (the structural insight — no SMOs, bounded structural work, wait-free O(1) reads, native ordered range scans — is real and is the correct family for this constraint set); (ii) the bcachefs-style IN-KEY snapshot id (bounded, needs none of Wei-et-al). DROP: the Wei-et-al versioned-CAS whole-tree snapshot (its snapshot-read cost is the unbounded smoking gun and it is redundant given in-k

### Learned lock-free index core (frontier / long-shot) — `unsuitable`

- **Core:** A shallow, intentionally UNBALANCED hierarchy of immutable piecewise-linear models over a sorted key array, with lock-free "bins" absorbing inserts/deletes, plus per-key versioned-value (vValue) chains and a global timestamp for snapshot/range reads. (Kanva = the only published lock-free learned ind
- **Best at:** For DENSE near-uniform FS keys (inode#, block offset, extent ranges), the learned core turns the read hot path into O(1) cache-light model arithmetic with the lowest LLC-miss count of any structure measured, AND does it with NO global rebalancing — which is si
- **Fatal flaw (4-way):** The two properties an FS core needs most — lock-freedom and crash-consistency — exist only in DIFFERENT, mutually incompatible members of this family (lock-free Kanva has no durability story; durable APEX is lock-based PMem). The proposed escape — run the learned layer as a VOLATILE accelerator over a separately journalled/CoW authoritative store rebuilt at mount — silently relocates the ENTIRE FS-core problem (ordered, range-scannable, snapshot-capable, crash-consistent) int
- **Bottom line:** Honest bottom line: this clears exactly ONE of the four bars (lock-free / wait-free reads, where it genuinely beats bcachefs's OLC) and is competitive on range/snapshot semantics, but fails soft-realtime and crash-consistency, and those two failures are the same structural fact rather than independent bugs. To make it work you would need to compose four things no one has demonstrated together on a learned layout: (1) wait-free helping for the retrain SMO with a HARD small-bin cap, (2) Crystalline-style bounded wait

---

## Synthesis: the four constraints *are* jointly satisfiable — but only as a hybrid, not an off-the-shelf design

The verdicts were rendered **per isolated design**. Read together, the two survivors fail at **complementary** points, and every "unsuitable" failure is a *requirement-too-strong* or *wrong-reclamation* problem that a known mechanism fixes:

| Failure seen in isolation | Why it happened | The routing-around fix (paper-grounded) |
|---|---|---|
| ART: wait-free reads ✗ durable-linearizability (PACTree dropped non-blocking reads) | the index lived **in PMEM**, so a read could observe a not-yet-persisted store | keep the **authoritative index in DRAM**, make durability a **separate journal** (NBTree's reconstructable layer; bcachefs's journalled-btree). Reads are wait-free on DRAM; a key becomes *visible to a new snapshot* only after its journal entry commits → the collision dissolves |
| CoW radix: single **root-CAS serializes all writers** | one global commit point | **per-node CoW + async SMO-log** (PACTree) instead of one root swap → writers touch ≤2 local nodes, structural publish is off the critical path → per-subtree concurrency like bcachefs |
| LSM / B-skiplist / learned: **unbounded tail** | epoch reclamation + compaction/helping/retrain | **Crystalline wait-free reclamation** (bounded, 3 words/obj) replaces epochs — the *one change every agent flagged as mandatory* |
| range scans can't be wait-free + bounded (Ko-2025: linearizable predecessor forces helping) | requirement set **too strong** | a filesystem needs **snapshot-consistent** dir-enum/extent scans, **not** live-linearizable predecessor → read-at-snapshot-id sidesteps helping entirely |
| Ctrie: hash order kills range scans; Wei-et-al snapshots unbounded | wrong trie + wrong snapshot mechanism | **MSB-radix (ordered) trie** + **bcachefs in-key snapshot id** (bounded) instead of versioned-CAS |

### The composite design

**An ordered, DRAM-authoritative Adaptive Radix Trie, journalled for durability, with Crystalline reclamation and in-key snapshots.** Concretely, mapped to the four constraints:

1. **Lock-free (beats bcachefs's OLC):** ROWEX/lock-free ART → **wait-free, O(1)-bounded, comparison-free reads** (depth is constant for fixed-width `inode||offset||snap` keys). Writers are **lock-free with bounded local work** (single-node CoW resize ≤2 nodes; local leaf split spliced into a sibling list; structural insert into the trie deferred to an async SMO-log). Strictly better progress than OLC; honest claim = *wait-free reads + lock-free bounded-latency writes*.
2. **Soft-realtime:** no propagating SMOs (radix) ⇒ bounded structural work; **Crystalline** ⇒ bounded reclamation; **snapshot-consistent range scans** ⇒ no helping; async SMO-log ⇒ structural publish off the writer's critical path. The residual soft bound (search-layer lag → ≤1 sibling hop, PACTree-measured 99%+) is covered by a bounded-lag SLO + reader re-descend fallback.
3. **Crash-consistency + O(1) snapshots:** a **lock-free append journal** is the durability authority; the DRAM trie is replayed/reconstructed on mount (NBTree/bcachefs model — fast, and it is *why* wait-free reads survive). **In-key snapshot ids** give O(1) bounded snapshots; a snapshot read radix-range-scans the snapshot sub-key for the visible ancestor version.
4. **Performance ≥ bcachefs:** radix point lookups beat a B+tree (constant hops, no key compares, PACTree-measured ~3.8× less read bandwidth, ~3.2× throughput, ~20× better p99.9); range scans match bcachefs (the **data layer is a slotted-leaf linked list** = bcachefs-class scan locality); write-batching parity comes from the same journal + write-buffer bcachefs uses, over **large slotted leaves**.

### What is genuinely novel / unbuilt (the honest caveat)

No system combines all of this end-to-end today. The unsolved-in-one-place integration is: **(a)** lock-free per-node CoW ART + Crystalline + journal recovery as a single proven stack; **(b)** the async SMO-log's bounded-lag guarantee under adversarial write bursts; **(c)** making *writes* wait-free rather than merely lock-free (reads are the easy win; bounded-latency wait-free writers on a radix trie remain open). So the deliverable is a **research design with three concrete open problems**, not a drop-in — but it is the *only* family where the four constraints provably co-exist, and each open problem has a paper-grounded starting point in this library.

### Tie-back to `portable-collections`

This is the same conclusion the workspace's own roadmap reached from the other direction: the **`RadixBimap` / ART-backed** experiment (dense small-integer ids → flat-`Vec` radix). The FS exploration **validates radix as the strategic core**, and the workspace can prototype the hard parts that carry no kernel/crash-consistency burden — the ordered-radix structure, wait-free reads, snapshot-consistent scans, and Crystalline-style reclamation — **in userspace first**, exactly where this analysis says the risk is lowest.
