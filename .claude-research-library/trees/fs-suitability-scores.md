# Filesystem-suitability scores — `trees/` research library

Per-paper suitability for use **inside a file system**, scored 0–5 on seven use-case dimensions. Generated 2026-06-21 by reading each paper's abstract + introduction and applying a fixed rubric (one scoring pass per paper).

**Scale** — 5 canonical/direct fit · 4 strong · 3 moderate (ideas transfer) · 2 weak · 1 minimal · 0 not applicable.

**Dimensions** —
- **MetaIdx** — authoritative on-disk metadata B-tree (inode/catalog/extent/dir index)
- **Durab** — crash-consistency / failure-atomic recovery (CoW, journaling, failure-atomic ops)
- **WrOpt** — write-amplification reduction / SSD-flash-zoned endurance
- **Concur** — concurrency for multi-threaded/kernel FS access (lock-free, RCU, OLC)
- **Cache** — in-DRAM page-cache / metadata-cache index (volatile)
- **Scan** — ordered iteration / range scan (directory enumeration, extent maps)
- **Snap** — snapshots / clones / versioning (CoW)
- **Σ** — holistic overall filesystem suitability (0–5; not a sum)

---

## ★ B+tree-weighted view — `(K, Σ)` tuples

Because the project target is specifically a **B+tree**, the headline ranking is a two-axis **tuple** rather than a blended score (a weighted sum would flatten a kinship-5/Σ-3 paper and a kinship-2/Σ-5 paper onto the same number):

- **K** = structural **kinship to a B+tree** (0–5): 5 = is a B+tree / direct variant (B-link, Bε-tree, Bw-tree, BzTree, CSB+, …); 4 = a B-tree / (a,b)-tree / CoW B-tree kin; 3 = hybrid cousin (trie-of-B+trees, COLA/fractal); 2 = other ordered family (skip list, ART, BST); 1 = structure-agnostic primitive (reclamation, WAL, FTL, verification); 0 = non-B+tree (LSM runs, Merkle/block-pointer tree).
- **Σ** = filesystem use-case suitability (0–5), from the per-dimension scores below.

Ordering is **lexicographic: K ↓ then Σ ↓** — B+tree-ness is the primary key (honoring the project's emphasis), FS suitability the tiebreaker. No arbitrary weight.

### Quadrant map (counts)

Rows = B+tree kinship K (5 → 0); columns = FS suitability Σ (0 → 5). Each cell = number of papers.

| K \ Σ | 0 | 1 | 2 | 3 | 4 | 5 | row Σ |
|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|
| **5** B+tree | · | · | 4 | 4 | 1 | 12 | 21 |
| **4** B-tree kin | · | · | · | · | · | 3 | 3 |
| **3** cousin | · | · | · | · | · | 2 | 2 |
| **2** other tree | · | 1 | 5 | 8 | 1 | · | 15 |
| **1** primitive | · | 3 | 13 | 9 | 5 | 1 | 31 |
| **0** non-tree | · | · | · | · | 3 | 4 | 7 |

> The **top-right block (K≥4, Σ≥4)** is the bullseye: a structure that is a B+tree *and* fits inside a filesystem. The **bottom-right** (K≤3, Σ≥4) are FS-strong but not B+trees — the K axis intentionally demotes them. The **top-left** (K≥4, Σ≤3) are real B+trees that lack durability (in-memory).

### Quadrants

**Q1 · B+tree **and** FS-ready (bullseye)** — 16 papers
- `(5,5)` *bcachefs: Principles of Operation* — B+tree (CoW, large log-structured nodes / append-only update vectors)
- `(5,5)` *The Bw-Tree: A B-tree for New Hardware Platforms* — B+tree (latch-free, mapping-table + delta records, CAS)
- `(5,5)` *How to Copy Files* — Be-tree (B-epsilon-tree) — write-optimized B+tree (BetrFS full-path-indexed FS)
- `(5,5)` *BzTree: A High-Performance Latch-free Range Index for Non…* — B+tree (latch-free, PMwCAS-based — BzTree)
- `(5,5)` *The Bw-Tree: A Latch-Free B-Tree for Log-Structured Flash…* — B+tree (latch-free, mapping-table + delta records)
- `(5,5)` *NBTree: a Lock-free PM-friendly Persistent B+-Tree for eA…* — B+tree (lock-free, persistent-memory; two-layer leaves + CAS in-place updates)
- `(5,5)` *Optimizing Every Operation in a Write-optimized File Syst…* — Bε-tree (B-epsilon-tree, write-optimized) as the file-system index (BetrFS 0.2)
- `(5,5)` *Endurable Transient Inconsistency in Byte-Addressable Per…* — persistent B+tree (byte-addressable PM, FAST/FAIR failure-atomic shift + in-place rebalance, latch-free reads)
- `(5,5)` *The Full Path to Full-Path Indexing* — Bε-tree (B-epsilon-tree, write-optimized B+tree variant)
- `(5,5)` *BetrFS: Write-Optimization in a Kernel File System* — Bε-tree (B-epsilon-tree, write-optimized; Fractal/TokuDB lineage) as the kernel FS on-disk index
- `(5,5)` *NV-Tree: A Consistent and Workload-adaptive Tree Structur…* — persistent B+tree (NVM, append-only unsorted leaves + reconstructable inner nodes)
- `(5,5)` *BetrFS: A Right-Optimized Write-Optimized File System* — Be-tree (B-epsilon-tree, write-optimized B+tree variant)
- `(5,4)` *Efficient Locking for Concurrent Operations on B-Trees* — B+tree (B-link tree: right-sibling links + high keys)
- `(4,5)` *B-trees, Shadowing, and Clones* — copy-on-write B+tree (shadowing, btrfs-style, with writable clones)
- `(4,5)` *Btrfs: The Swiss Army Knife of Storage* — copy-on-write B-tree (btrfs, Ohad Rodeh shadowing)
- `(4,5)` *APFS Internals for Forensic Analysis (ERNW Whitepaper 65)* — copy-on-write B-tree (APFS object/omap B-tree)

**Q2 · B+tree, weak FS fit (in-memory / needs durability)** — 8 papers
- `(5,3)` *Elimination (a,b)-trees with fast, durable updates* — concurrent (a,b)-tree / B+tree (OCC-ABtree, Elim-ABtree; optimistic concurrency + elimination)
- `(5,3)` *Building a Bw-Tree Takes More Than Just Buzz Words* — B+tree (Bw-tree: latch-free, mapping-table + delta chains)
- `(5,3)` *A Lock-Free B+tree* — B+tree (lock-free, CAS-based, chunk-mechanism nodes)
- `(5,3)` *Cache-Conscious Concurrency Control of Main-Memory Indexe…* — B+tree (cache-conscious: B+-tree / CSB+-tree) with OLFIT latch-free concurrency control
- `(5,2)` *PALM: Parallel Architecture-Friendly Latch-Free Modificat…* — B+tree (latch-free, BSP-batched concurrent queries + SIMD in-node search)
- `(5,2)` *FB+-tree: A Memory-Optimized B+-tree with Latch-Free Upda…* — B+tree (main-memory, latch-free, B-link + optimistic lock, trie-blended feature comparison)
- `(5,2)` *BS-tree: A gapped data-parallel B-tree* — B+tree (in-memory, SIMD/cache-conscious: gapped nodes + duplicated keys + FOR compression)
- `(5,2)` *ELB-Trees, An Efficient and Lock-free B-tree Derivative* — B+tree (lock-free, leaf-oriented k-ary search tree)

**Q3 · FS-strong, **not** a B+tree (demoted by the K axis)** — 16 papers
- PACTree: A High Performance Persistent … `(3,5)`, The TokuFS Streaming File System `(3,5)`, Evaluating Persistent Memory Range Inde… `(2,4)`, Evaluating Persistent Memory Range Inde… `(1,5)`, Easy Lock-Free Indexing in Non-Volatile… `(1,4)`, DFTL: A Flash Translation Layer Employi… `(1,4)`, ARIES: A Transaction Recovery Method Su… `(1,4)`, Soft Updates: A Solution to the Metadat… `(1,4)`, Optimistic Crash Consistency `(1,4)`, File System Design for an NFS File Serv… `(0,5)`, F2FS: A New File System for Flash Stora… `(0,5)`, The Zettabyte File System `(0,5)`, The Design and Implementation of a Log-… `(0,5)`, LSM-based Storage Techniques: A Survey `(0,4)`, WiscKey: Separating Keys from Values in… `(0,4)`, PebblesDB: Building Key-Value Stores us… `(0,4)`

**Q4 · peripheral (supporting primitives / different family)** — 39 papers
- Persistent Non-Blocking Binary Search T… `(2,3)`, Concurrent Balanced Augmented Trees `(2,3)`, Bridging Cache-Friendliness and Concurr… `(2,3)`, Learned Lock-free Search Data Structures `(2,3)`, The ART of Practical Synchronization `(2,3)`, A General Technique for Non-blocking Tr… `(2,3)`, Non-blocking Binary Search Trees `(2,3)`, Non-blocking k-ary Search Trees `(2,3)`, Skip Hash: A Fast Ordered Map Via Softw… `(2,2)`, A Provably Correct Scalable Concurrent … `(2,2)`, SALI: A Scalable Adaptive Learned Index… `(2,2)`, Efficient Lock-free Binary Search Trees `(2,2)`, A Lock-free Binary Trie `(2,2)`, Verifying Lock-free Search Structure Te… `(2,1)`, Persistent Memory I/O Primitives `(1,3)`, Practical Persistent Multi-Word Compare… `(1,3)`, Practical Lock-Freedom (UCAM-CL-TR-579) `(1,3)`, Guidelines for Building Indexes on Part… `(1,3)`, Pragmatic Primitives for Non-blocking D… `(1,3)`, Reclaiming Memory for Lock-Free Data St… `(1,3)`, Analysis and Evolution of Journaling Fi… `(1,3)`, Making Lockless Synchronization Fast: P… `(1,3)`, ZNS: Avoiding the Block Interface Tax f… `(1,3)`, FreSh: A Lock-Free Data Series Index `(1,2)`, Interval-Based Memory Reclamation `(1,2)`, Brief Announcement: Hazard Eras - Non-B… `(1,2)`, Applying Hazard Pointers to More Concur… `(1,2)`, Publish on Ping: A Better Way to Publis… `(1,2)`, Verifying Concurrent Multicopy Search S… `(1,2)`, Are Your Epochs Too Epic? Batch Free Ca… `(1,2)`, A new and five older Concurrent Memory … `(1,2)`, Verifying Concurrent Search Structure T… `(1,2)`, Hazard Pointers: Safe Memory Reclamatio… `(1,2)`, NBR: Neutralization Based Reclamation `(1,2)`, Crystalline: Fast and Memory Efficient … `(1,2)`, All File Systems Are Not Created Equal:… `(1,2)`, Proving Highly-Concurrent Traversals Co… `(1,1)`, Performance Anomalies in Concurrent Dat… `(1,1)`, Verifying Linearizability: A Comparativ… `(1,1)`

### Full tuple matrix (lexicographic: K ↓, Σ ↓)

| # | (K,Σ) | Paper | Core structure | Quad |
|--:|:--:|---|---|:--:|
| 1 | **(5,5)** | bcachefs: Principles of Operation | B+tree (CoW, large log-structured nodes / append-only  | Q1 |
| 2 | **(5,5)** | The Bw-Tree: A B-tree for New Hardware Platforms | B+tree (latch-free, mapping-table + delta records, CAS | Q1 |
| 3 | **(5,5)** | How to Copy Files | Be-tree (B-epsilon-tree) — write-optimized B+tree (Bet | Q1 |
| 4 | **(5,5)** | BzTree: A High-Performance Latch-free Range Index f… | B+tree (latch-free, PMwCAS-based — BzTree) | Q1 |
| 5 | **(5,5)** | The Bw-Tree: A Latch-Free B-Tree for Log-Structured… | B+tree (latch-free, mapping-table + delta records) | Q1 |
| 6 | **(5,5)** | NBTree: a Lock-free PM-friendly Persistent B+-Tree … | B+tree (lock-free, persistent-memory; two-layer leaves | Q1 |
| 7 | **(5,5)** | Optimizing Every Operation in a Write-optimized Fil… | Bε-tree (B-epsilon-tree, write-optimized) as the file- | Q1 |
| 8 | **(5,5)** | Endurable Transient Inconsistency in Byte-Addressab… | persistent B+tree (byte-addressable PM, FAST/FAIR fail | Q1 |
| 9 | **(5,5)** | The Full Path to Full-Path Indexing | Bε-tree (B-epsilon-tree, write-optimized B+tree varian | Q1 |
| 10 | **(5,5)** | BetrFS: Write-Optimization in a Kernel File System | Bε-tree (B-epsilon-tree, write-optimized; Fractal/Toku | Q1 |
| 11 | **(5,5)** | NV-Tree: A Consistent and Workload-adaptive Tree St… | persistent B+tree (NVM, append-only unsorted leaves +  | Q1 |
| 12 | **(5,5)** | BetrFS: A Right-Optimized Write-Optimized File Syst… | Be-tree (B-epsilon-tree, write-optimized B+tree varian | Q1 |
| 13 | **(5,4)** | Efficient Locking for Concurrent Operations on B-Tr… | B+tree (B-link tree: right-sibling links + high keys) | Q1 |
| 14 | **(5,3)** | Elimination (a,b)-trees with fast, durable updates | concurrent (a,b)-tree / B+tree (OCC-ABtree, Elim-ABtre | Q2 |
| 15 | **(5,3)** | Building a Bw-Tree Takes More Than Just Buzz Words | B+tree (Bw-tree: latch-free, mapping-table + delta cha | Q2 |
| 16 | **(5,3)** | A Lock-Free B+tree | B+tree (lock-free, CAS-based, chunk-mechanism nodes) | Q2 |
| 17 | **(5,3)** | Cache-Conscious Concurrency Control of Main-Memory … | B+tree (cache-conscious: B+-tree / CSB+-tree) with OLF | Q2 |
| 18 | **(5,2)** | PALM: Parallel Architecture-Friendly Latch-Free Mod… | B+tree (latch-free, BSP-batched concurrent queries + S | Q2 |
| 19 | **(5,2)** | FB+-tree: A Memory-Optimized B+-tree with Latch-Fre… | B+tree (main-memory, latch-free, B-link + optimistic l | Q2 |
| 20 | **(5,2)** | BS-tree: A gapped data-parallel B-tree | B+tree (in-memory, SIMD/cache-conscious: gapped nodes  | Q2 |
| 21 | **(5,2)** | ELB-Trees, An Efficient and Lock-free B-tree Deriva… | B+tree (lock-free, leaf-oriented k-ary search tree) | Q2 |
| 22 | **(4,5)** | B-trees, Shadowing, and Clones | copy-on-write B+tree (shadowing, btrfs-style, with wri | Q1 |
| 23 | **(4,5)** | Btrfs: The Swiss Army Knife of Storage | copy-on-write B-tree (btrfs, Ohad Rodeh shadowing) | Q1 |
| 24 | **(4,5)** | APFS Internals for Forensic Analysis (ERNW Whitepap… | copy-on-write B-tree (APFS object/omap B-tree) | Q1 |
| 25 | **(3,5)** | PACTree: A High Performance Persistent Range Index … | trie-of-B+trees hybrid (PDL-ART internal search layer  | Q3 |
| 26 | **(3,5)** | The TokuFS Streaming File System | Fractal Tree index (write-optimized streaming B-tree / | Q3 |
| 27 | **(2,4)** | Evaluating Persistent Memory Range Indexes | benchmark/evaluation methodology (PiBench) for B+tree- | Q3 |
| 28 | **(2,3)** | Persistent Non-Blocking Binary Search Trees Support… | binary search tree (lock-free / non-blocking, persiste | Q4 |
| 29 | **(2,3)** | Concurrent Balanced Augmented Trees | balanced binary search tree (lock-free chromatic/augme | Q4 |
| 30 | **(2,3)** | Bridging Cache-Friendliness and Concurrency: A Loca… | B-skiplist (blocked/cache-optimized skip list) | Q4 |
| 31 | **(2,3)** | Learned Lock-free Search Data Structures | learned index (lock-free; hierarchy of linear models o | Q4 |
| 32 | **(2,3)** | The ART of Practical Synchronization | radix trie / ART (Adaptive Radix Tree), synchronized v | Q4 |
| 33 | **(2,3)** | A General Technique for Non-blocking Trees | binary search tree (lock-free, relaxed-balance chromat | Q4 |
| 34 | **(2,3)** | Non-blocking Binary Search Trees | binary search tree (lock-free, leaf-oriented, CAS-only | Q4 |
| 35 | **(2,3)** | Non-blocking k-ary Search Trees | k-ary search tree (lock-free, leaf-oriented; generaliz | Q4 |
| 36 | **(2,2)** | Skip Hash: A Fast Ordered Map Via Software Transact… | skip list + hash map composite (skip hash, STM-based) | Q4 |
| 37 | **(2,2)** | A Provably Correct Scalable Concurrent Skip List | skip list (concurrent, optimistic locking) | Q4 |
| 38 | **(2,2)** | SALI: A Scalable Adaptive Learned Index Framework b… | learned index (RMI/LIPP-style model hierarchy over sor | Q4 |
| 39 | **(2,2)** | Efficient Lock-free Binary Search Trees | binary search tree (lock-free, internal) | Q4 |
| 40 | **(2,2)** | A Lock-free Binary Trie | binary trie / radix trie (lock-free, ordered predecess | Q4 |
| 41 | **(2,1)** | Verifying Lock-free Search Structure Templates | skip list (lock-free) + linked lists; via Iris lineari | Q4 |
| 42 | **(1,5)** | Evaluating Persistent Memory Range Indexes: Part Tw… | benchmark/survey (evaluation of PM range indexes; mixe | Q3 |
| 43 | **(1,4)** | Easy Lock-Free Indexing in Non-Volatile Memory | PMwCAS (persistent multi-word compare-and-swap primiti | Q3 |
| 44 | **(1,4)** | DFTL: A Flash Translation Layer Employing Demand-ba… | FTL mapping table (demand-cached page-level address tr | Q3 |
| 45 | **(1,4)** | ARIES: A Transaction Recovery Method Supporting Fin… | WAL/journaling protocol (ARIES redo-undo recovery) | Q3 |
| 46 | **(1,4)** | Soft Updates: A Solution to the Metadata Update Pro… | WAL/journaling protocol (crash-consistency: dependency | Q3 |
| 47 | **(1,4)** | Optimistic Crash Consistency | WAL/journaling protocol (optimistic crash consistency) | Q3 |
| 48 | **(1,3)** | Persistent Memory I/O Primitives | PMem I/O primitives (log writing + block flushing; not | Q4 |
| 49 | **(1,3)** | Practical Persistent Multi-Word Compare-and-Swap Al… | PMwCAS atomic primitive (multi-word CAS for persistent | Q4 |
| 50 | **(1,3)** | Practical Lock-Freedom (UCAM-CL-TR-579) | memory-reclamation scheme (not a structure) — EBR + lo | Q4 |
| 51 | **(1,3)** | Guidelines for Building Indexes on Partially Cache-… | CXL partial-cache-coherence concurrency-control guidel | Q4 |
| 52 | **(1,3)** | Pragmatic Primitives for Non-blocking Data Structur… | LLX/SCX/VLX multi-word LL/SC/VL synchronization primit | Q4 |
| 53 | **(1,3)** | Reclaiming Memory for Lock-Free Data Structures: Th… | memory-reclamation scheme (not a structure) — DEBRA, a | Q4 |
| 54 | **(1,3)** | Analysis and Evolution of Journaling File Systems | WAL/journaling protocol (crash-consistency analysis me | Q4 |
| 55 | **(1,3)** | Making Lockless Synchronization Fast: Performance I… | memory-reclamation scheme (not a structure) | Q4 |
| 56 | **(1,3)** | ZNS: Avoiding the Block Interface Tax for Flash-bas… | FTL mapping table / zoned-storage interface (not a tre | Q4 |
| 57 | **(1,2)** | FreSh: A Lock-Free Data Series Index | iSAX-based data series index (SAX-summarization tree)  | Q4 |
| 58 | **(1,2)** | Interval-Based Memory Reclamation | memory-reclamation scheme (not a structure) | Q4 |
| 59 | **(1,2)** | Brief Announcement: Hazard Eras - Non-Blocking Memo… | memory-reclamation scheme (not a structure) | Q4 |
| 60 | **(1,2)** | Applying Hazard Pointers to More Concurrent Data St… | memory-reclamation scheme (not a structure) | Q4 |
| 61 | **(1,2)** | Publish on Ping: A Better Way to Publish Reservatio… | memory-reclamation scheme (not a structure) | Q4 |
| 62 | **(1,2)** | Verifying Concurrent Multicopy Search Structures | linearizability verification method (multicopy search  | Q4 |
| 63 | **(1,2)** | Are Your Epochs Too Epic? Batch Free Can Be Harmful | memory-reclamation scheme (not a structure) — EBR / am | Q4 |
| 64 | **(1,2)** | A new and five older Concurrent Memory Reclamation … | memory-reclamation scheme (not a structure) | Q4 |
| 65 | **(1,2)** | Verifying Concurrent Search Structure Templates | linearizability verification method (concurrent search | Q4 |
| 66 | **(1,2)** | Hazard Pointers: Safe Memory Reclamation for Lock-F… | memory-reclamation scheme (not a structure) | Q4 |
| 67 | **(1,2)** | NBR: Neutralization Based Reclamation | memory-reclamation scheme (not a structure) | Q4 |
| 68 | **(1,2)** | Crystalline: Fast and Memory Efficient Wait-Free Re… | memory-reclamation scheme (not a structure) — wait-fre | Q4 |
| 69 | **(1,2)** | All File Systems Are Not Created Equal: On the Comp… | crash-consistency / persistence-property testing metho | Q4 |
| 70 | **(1,1)** | Proving Highly-Concurrent Traversals Correct | linearizability verification method (proof technique f | Q4 |
| 71 | **(1,1)** | Performance Anomalies in Concurrent Data Structure … | benchmark/survey (concurrent CSet microbenchmark metho | Q4 |
| 72 | **(1,1)** | Verifying Linearizability: A Comparative Survey | linearizability verification method (comparative surve | Q4 |
| 73 | **(0,5)** | File System Design for an NFS File Server Appliance | Merkle/block-pointer tree (WAFL copy-on-write inode/in | Q3 |
| 74 | **(0,5)** | F2FS: A New File System for Flash Storage | Log-structured file system with Node Address Table (NA | Q3 |
| 75 | **(0,5)** | The Zettabyte File System | Merkle / block-pointer tree (copy-on-write, self-valid | Q3 |
| 76 | **(0,5)** | The Design and Implementation of a Log-Structured F… | Log-structured file system (append-only log + inode-ma | Q3 |
| 77 | **(0,4)** | LSM-based Storage Techniques: A Survey | LSM-tree (survey/taxonomy of sorted-run merge storage) | Q3 |
| 78 | **(0,4)** | WiscKey: Separating Keys from Values in SSD-conscio… | LSM-tree (key-value separated; values in a log) | Q3 |
| 79 | **(0,4)** | PebblesDB: Building Key-Value Stores using Fragment… | LSM-tree (Fragmented LSM / FLSM, guard-based sorted ru | Q3 |

---

## A. Lock-free / latch-free B+trees (in-memory core)

| Paper | MetaIdx | Durab | WrOpt | Concur | Cache | Scan | Snap | Σ | Best-fit use |
|---|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|---|
| The Bw-Tree: A B-tree for New Hardware Platforms | 5 | 3 | 4 | 5 | 5 | 5 | 1 | **5** | latch-free B-tree index (on-disk metadata + in-DRAM cache) for multi-core/kernel FS access |
| BzTree: A High-Performance Latch-free Range Index for Non… | 5 | 5 | 2 | 5 | 4 | 5 | 1 | **5** | persistent latch-free metadata B-tree index (on NVM, with failure-atomic recovery) |
| The Bw-Tree: A Latch-Free B-Tree for Log-Structured Flash… | 5 | 2 | 4 | 5 | 5 | 4 | 1 | **5** | latch-free B+tree metadata index (CAS-only concurrent index with mapping-table indirection) |
| Building a Bw-Tree Takes More Than Just Buzz Words | 2 | 0 | 2 | 5 | 5 | 3 | 1 | **3** | in-DRAM concurrent metadata-cache index (lock-free) with epoch-based reclamation |
| A Lock-Free B+tree | 1 | 0 | 0 | 5 | 4 | 4 | 0 | **3** | lock-free concurrency mechanism for an in-DRAM ordered metadata index |
| PALM: Parallel Architecture-Friendly Latch-Free Modificat… | 1 | 0 | 1 | 5 | 4 | 3 | 1 | **2** | concurrency mechanism for an in-DRAM B+tree metadata cache |
| FB+-tree: A Memory-Optimized B+-tree with Latch-Free Update | 1 | 0 | 0 | 5 | 5 | 4 | 0 | **2** | in-DRAM metadata-cache index with latch-free concurrent access |
| ELB-Trees, An Efficient and Lock-free B-tree Derivative | 1 | 0 | 0 | 5 | 3 | 3 | 0 | **2** | concurrency mechanism for an in-DRAM concurrent index |

## B. Concurrent B-tree synchronization techniques

| Paper | MetaIdx | Durab | WrOpt | Concur | Cache | Scan | Snap | Σ | Best-fit use |
|---|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|---|
| Efficient Locking for Concurrent Operations on B-Trees | 4 | 1 | 1 | 5 | 3 | 3 | 0 | **4** | concurrency mechanism for a multi-threaded/kernel FS B+tree index (B-link tree) |
| Elimination (a,b)-trees with fast, durable updates | 3 | 3 | 3 | 5 | 4 | 2 | 0 | **3** | concurrency mechanism for a multi-threaded/kernel FS metadata B-tree |
| Cache-Conscious Concurrency Control of Main-Memory Indexe… | 1 | 0 | 0 | 5 | 4 | 3 | 1 | **3** | concurrency mechanism for a near-lock-free in-DRAM B+tree index |
| The ART of Practical Synchronization | 1 | 0 | 0 | 5 | 4 | 3 | 1 | **3** | concurrency control for an in-DRAM ordered metadata index (Optimistic Lock Coupling / ROWEX) |

## C. Persistent / NVM lock-free B+trees & primitives

| Paper | MetaIdx | Durab | WrOpt | Concur | Cache | Scan | Snap | Σ | Best-fit use |
|---|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|---|
| NBTree: a Lock-free PM-friendly Persistent B+-Tree for eA… | 5 | 5 | 4 | 5 | 4 | 3 | 0 | **5** | lock-free crash-consistent persistent B+-tree metadata index (with DRAM-cached inner/metadata layer) |
| PACTree: A High Performance Persistent Range Index Using … | 5 | 4 | 4 | 5 | 2 | 5 | 0 | **5** | on-disk/persistent metadata range index with concurrent SMOs |
| Endurable Transient Inconsistency in Byte-Addressable Per… | 5 | 5 | 3 | 4 | 2 | 4 | 1 | **5** | crash-consistency layer for a persistent (PM/NVM) on-device metadata B+-tree |
| NV-Tree: A Consistent and Workload-adaptive Tree Structur… | 5 | 5 | 4 | 2 | 2 | 4 | 0 | **5** | crash-consistency layer (persistent on-NVM B+tree metadata index) |
| Easy Lock-Free Indexing in Non-Volatile Memory | 3 | 5 | 2 | 5 | 3 | 0 | 1 | **4** | failure-atomic multi-word update primitive for a lock-free persistent (NVM) index |
| Persistent Memory I/O Primitives | 1 | 5 | 4 | 2 | 3 | 0 | 2 | **3** | crash-consistency layer (failure-atomic NVM log + CoW/micro-log page flushing primitives) |
| Practical Persistent Multi-Word Compare-and-Swap Algorith… | 1 | 4 | 3 | 5 | 2 | 0 | 0 | **3** | memory reclamation/atomics primitive for a lock-free persistent B+tree |

## D. Non-blocking ordered trees & alternatives

| Paper | MetaIdx | Durab | WrOpt | Concur | Cache | Scan | Snap | Σ | Best-fit use |
|---|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|---|
| Persistent Non-Blocking Binary Search Trees Supporting Wa… | 1 | 1 | 0 | 5 | 4 | 5 | 3 | **3** | concurrent in-DRAM metadata index with wait-free ordered range scans |
| Concurrent Balanced Augmented Trees | 1 | 0 | 0 | 5 | 4 | 4 | 3 | **3** | concurrency mechanism for a lock-free in-DRAM ordered metadata index |
| Bridging Cache-Friendliness and Concurrency: A Locality-O… | 1 | 0 | 1 | 5 | 5 | 4 | 0 | **3** | in-DRAM concurrent ordered metadata-cache index |
| A General Technique for Non-blocking Trees | 1 | 0 | 0 | 5 | 4 | 3 | 1 | **3** | concurrency mechanism for a lock-free in-DRAM balanced/ordered index |
| Non-blocking Binary Search Trees | 1 | 1 | 0 | 5 | 3 | 2 | 1 | **3** | concurrency mechanism for a lock-free in-DRAM ordered metadata index |
| Non-blocking k-ary Search Trees | 1 | 0 | 0 | 5 | 4 | 2 | 0 | **3** | lock-free concurrency mechanism for an in-DRAM metadata index |
| Pragmatic Primitives for Non-blocking Data Structures | 0 | 1 | 0 | 5 | 2 | 1 | 1 | **3** | concurrency primitive for lock-free in-DRAM tree node updates (atomic split/merge) |
| A Provably Correct Scalable Concurrent Skip List | 1 | 0 | 0 | 5 | 4 | 4 | 0 | **2** | concurrency mechanism for an in-DRAM ordered metadata cache (lock-free/optimistic ordered index) |
| Efficient Lock-free Binary Search Trees | 1 | 0 | 0 | 5 | 3 | 2 | 0 | **2** | concurrency mechanism (lock-free single-word-CAS index with disjoint-access-parallelism) for an in-DRAM metadata cache |
| A Lock-free Binary Trie | 0 | 0 | 0 | 5 | 3 | 3 | 0 | **2** | concurrency mechanism for a lock-free ordered in-DRAM index |

## E. Safe memory reclamation

| Paper | MetaIdx | Durab | WrOpt | Concur | Cache | Scan | Snap | Σ | Best-fit use |
|---|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|---|
| Practical Lock-Freedom (UCAM-CL-TR-579) | 1 | 1 | 0 | 5 | 4 | 3 | 1 | **3** | memory reclamation (EBR) for concurrent in-DRAM index |
| Reclaiming Memory for Lock-Free Data Structures: There ha… | 0 | 1 | 0 | 5 | 3 | 0 | 0 | **3** | memory reclamation for concurrent index |
| Making Lockless Synchronization Fast: Performance Implica… | 0 | 0 | 0 | 5 | 3 | 0 | 0 | **3** | memory reclamation for concurrent index |
| Interval-Based Memory Reclamation | 0 | 0 | 0 | 5 | 3 | 1 | 1 | **2** | memory reclamation for concurrent index |
| Brief Announcement: Hazard Eras - Non-Blocking Memory Rec… | 0 | 0 | 0 | 5 | 3 | 0 | 1 | **2** | memory reclamation for concurrent index |
| Applying Hazard Pointers to More Concurrent Data Structur… | 0 | 0 | 0 | 5 | 3 | 1 | 0 | **2** | memory reclamation for concurrent index |
| Publish on Ping: A Better Way to Publish Reservations in … | 0 | 0 | 0 | 5 | 3 | 1 | 0 | **2** | memory reclamation for concurrent index |
| Are Your Epochs Too Epic? Batch Free Can Be Harmful | 0 | 0 | 0 | 5 | 2 | 1 | 0 | **2** | memory reclamation for concurrent index |
| A new and five older Concurrent Memory Reclamation Scheme… | 0 | 1 | 0 | 5 | 2 | 0 | 0 | **2** | memory reclamation for concurrent index |
| Hazard Pointers: Safe Memory Reclamation for Lock-Free Ob… | 0 | 0 | 0 | 5 | 2 | 0 | 0 | **2** | memory reclamation for concurrent index |
| NBR: Neutralization Based Reclamation | 0 | 0 | 0 | 5 | 2 | 0 | 0 | **2** | memory reclamation for concurrent index |
| Crystalline: Fast and Memory Efficient Wait-Free Reclamation | 0 | 0 | 0 | 4 | 2 | 0 | 0 | **2** | memory reclamation for concurrent index |

## F. Modern / learned / specialized concurrent indexes

| Paper | MetaIdx | Durab | WrOpt | Concur | Cache | Scan | Snap | Σ | Best-fit use |
|---|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|---|
| Learned Lock-free Search Data Structures | 1 | 0 | 0 | 5 | 4 | 4 | 1 | **3** | in-DRAM concurrent (lock-free) metadata-cache index with range scans |
| Guidelines for Building Indexes on Partially Cache-Cohere… | 1 | 3 | 2 | 5 | 3 | 1 | 0 | **3** | concurrency control for a lock-free B+tree on shared (CXL/relaxed-coherence) memory |
| Skip Hash: A Fast Ordered Map Via Software Transactional … | 1 | 0 | 0 | 5 | 4 | 5 | 2 | **2** | in-DRAM concurrent ordered metadata-cache index with linearizable range scans |
| SALI: A Scalable Adaptive Learned Index Framework based o… | 1 | 0 | 1 | 5 | 4 | 3 | 0 | **2** | in-DRAM ordered index with high concurrent-insert scalability |
| BS-tree: A gapped data-parallel B-tree | 1 | 0 | 1 | 3 | 4 | 4 | 0 | **2** | in-DRAM (cache/SIMD-friendly) metadata index |
| FreSh: A Lock-Free Data Series Index | 1 | 0 | 0 | 5 | 3 | 2 | 0 | **2** | lock-free concurrency for an in-DRAM index |

## G. Surveys & benchmarks

| Paper | MetaIdx | Durab | WrOpt | Concur | Cache | Scan | Snap | Σ | Best-fit use |
|---|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|---|
| Evaluating Persistent Memory Range Indexes: Part Two [Ext… | 5 | 5 | 4 | 4 | 4 | 5 | 1 | **5** | on-disk/PM metadata index (durable lock-free B+tree backend selection) |
| Evaluating Persistent Memory Range Indexes | 4 | 4 | 3 | 4 | 3 | 4 | 1 | **4** | persistent on-NVM/PM metadata range index (evaluation/selection guidance) |
| Performance Anomalies in Concurrent Data Structure Microb… | 0 | 0 | 0 | 2 | 1 | 1 | 0 | **1** | n/a (benchmarking methodology only) |

## H. Formal verification / correctness

| Paper | MetaIdx | Durab | WrOpt | Concur | Cache | Scan | Snap | Σ | Best-fit use |
|---|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|---|
| Verifying Concurrent Multicopy Search Structures | 1 | 1 | 2 | 3 | 1 | 0 | 1 | **2** | n/a (verification/methodology only) |
| Verifying Concurrent Search Structure Templates | 1 | 0 | 0 | 4 | 2 | 1 | 0 | **2** | n/a (verification only) — concurrency-correctness proof recipe for a latch-free B-link tree |
| Verifying Lock-free Search Structure Templates | 0 | 0 | 0 | 3 | 1 | 1 | 0 | **1** | n/a (verification only) |
| Proving Highly-Concurrent Traversals Correct | 0 | 0 | 0 | 3 | 1 | 1 | 0 | **1** | n/a (verification only) — proving lock-free traversal correctness |
| Verifying Linearizability: A Comparative Survey | 0 | 0 | 0 | 2 | 0 | 0 | 0 | **1** | n/a (verification only) |

## I-1. Copy-on-write / shadow-paging B-trees

| Paper | MetaIdx | Durab | WrOpt | Concur | Cache | Scan | Snap | Σ | Best-fit use |
|---|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|---|
| bcachefs: Principles of Operation | 5 | 5 | 5 | 5 | 4 | 5 | 5 | **5** | write-optimized on-disk metadata B-tree (log-structured large nodes) for a CoW filesystem |
| B-trees, Shadowing, and Clones | 5 | 5 | 4 | 4 | 1 | 4 | 5 | **5** | crash-consistency CoW metadata index with writable snapshots (the btrfs tree) |
| Btrfs: The Swiss Army Knife of Storage | 5 | 5 | 4 | 1 | 1 | 4 | 5 | **5** | on-disk CoW metadata B-tree with snapshots/clones |
| APFS Internals for Forensic Analysis (ERNW Whitepaper 65) | 5 | 5 | 3 | 1 | 1 | 4 | 5 | **5** | on-disk CoW B-tree metadata index (with checkpoints + clones) |
| File System Design for an NFS File Server Appliance | 4 | 5 | 5 | 1 | 1 | 2 | 5 | **5** | crash-consistency / snapshot layer (shadow-paging CoW backbone) |
| The Zettabyte File System | 4 | 5 | 3 | 1 | 1 | 2 | 5 | **5** | crash-consistency layer (transactional copy-on-write shadow-paging FS) |

## I-2. Write-optimized (Bε-tree) file systems

| Paper | MetaIdx | Durab | WrOpt | Concur | Cache | Scan | Snap | Σ | Best-fit use |
|---|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|---|
| How to Copy Files | 5 | 5 | 5 | 1 | 2 | 5 | 5 | **5** | copy-on-write clones/snapshots in a write-optimized FS index |
| Optimizing Every Operation in a Write-optimized File System | 5 | 5 | 5 | 1 | 2 | 5 | 2 | **5** | write-optimized on-disk FS metadata/data index with crash-consistent journaling |
| The Full Path to Full-Path Indexing | 5 | 4 | 5 | 2 | 1 | 5 | 1 | **5** | on-disk full-path-keyed metadata/data index (write-optimized Be-tree) |
| BetrFS: Write-Optimization in a Kernel File System | 5 | 3 | 5 | 2 | 2 | 4 | 2 | **5** | write-amplification reduction (Bε-tree on-disk FS index) |
| The TokuFS Streaming File System | 5 | 2 | 5 | 2 | 2 | 5 | 1 | **5** | write-optimized on-disk FS metadata + data index |
| BetrFS: A Right-Optimized Write-Optimized File System | 5 | 3 | 5 | 1 | 2 | 4 | 1 | **5** | write-amplification reduction (write-optimized on-disk metadata/small-write index) |

## I-3. Log-structured, flash & zoned file systems

| Paper | MetaIdx | Durab | WrOpt | Concur | Cache | Scan | Snap | Σ | Best-fit use |
|---|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|---|
| F2FS: A New File System for Flash Storage | 5 | 5 | 5 | 2 | 1 | 2 | 2 | **5** | flash-optimized on-disk metadata indexing (NAT indirection) with log-structured write reduction |
| The Design and Implementation of a Log-Structured File Sy… | 4 | 5 | 5 | 0 | 1 | 1 | 2 | **5** | write-amplification reduction (log-structured sequential writes) |
| DFTL: A Flash Translation Layer Employing Demand-based Se… | 4 | 2 | 4 | 0 | 5 | 1 | 0 | **4** | demand-cached on-storage metadata mapping index (in-DRAM cache of a large persistent L2P index) |
| ZNS: Avoiding the Block Interface Tax for Flash-based SSDs | 1 | 2 | 5 | 0 | 0 | 0 | 0 | **3** | write-amplification reduction (append-only/zoned write contract) |

## I-4. Journaling / WAL & crash consistency

| Paper | MetaIdx | Durab | WrOpt | Concur | Cache | Scan | Snap | Σ | Best-fit use |
|---|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|---|
| ARIES: A Transaction Recovery Method Supporting Fine-Gran… | 1 | 5 | 3 | 4 | 2 | 0 | 1 | **4** | crash-consistency layer (write-ahead logging / journaling) |
| Soft Updates: A Solution to the Metadata Update Problem i… | 2 | 5 | 3 | 1 | 2 | 0 | 0 | **4** | crash-consistency layer (dependency-ordered metadata write-back) |
| Optimistic Crash Consistency | 1 | 5 | 3 | 1 | 0 | 0 | 1 | **4** | crash-consistency layer (optimistic journaling: ordering without flush) |
| Analysis and Evolution of Journaling File Systems | 2 | 4 | 2 | 1 | 0 | 0 | 0 | **3** | crash-consistency layer (journaling/WAL analysis) |
| All File Systems Are Not Created Equal: On the Complexity… | 0 | 3 | 2 | 0 | 0 | 0 | 1 | **2** | n/a (crash-consistency property characterization for FS-backed data structures) |

## I-5. Write-optimized key-value / LSM stores

| Paper | MetaIdx | Durab | WrOpt | Concur | Cache | Scan | Snap | Σ | Best-fit use |
|---|:--:|:--:|:--:|:--:|:--:|:--:|:--:|:--:|---|
| LSM-based Storage Techniques: A Survey | 4 | 3 | 5 | 3 | 2 | 4 | 2 | **4** | write-amplification reduction (write-optimized on-disk index backend) |
| WiscKey: Separating Keys from Values in SSD-conscious Sto… | 4 | 4 | 5 | 1 | 1 | 3 | 2 | **4** | write-amplification reduction (key-value separation for an SSD-conscious write-optimized FS index) |
| PebblesDB: Building Key-Value Stores using Fragmented Log… | 4 | 3 | 5 | 2 | 1 | 3 | 2 | **4** | write-amplification reduction (write-optimized on-disk KV/metadata store) |

---

## Top picks per use case (Σ-independent: highest scorers on each dimension)

**MetaIdx** — The Bw-Tree: A B-tree for New Hardware Pla (5); BzTree: A High-Performance Latch-free Rang (5); The Bw-Tree: A Latch-Free B-Tree for Log-S (5); NBTree: a Lock-free PM-friendly Persistent (5); NV-Tree: A Consistent and Workload-adaptiv (5)

**Durab** — BzTree: A High-Performance Latch-free Rang (5); NBTree: a Lock-free PM-friendly Persistent (5); NV-Tree: A Consistent and Workload-adaptiv (5); Endurable Transient Inconsistency in Byte- (5); Evaluating Persistent Memory Range Indexes (5)

**WrOpt** — File System Design for an NFS File Server  (5); bcachefs: Principles of Operation (5); BetrFS: A Right-Optimized Write-Optimized  (5); Optimizing Every Operation in a Write-opti (5); The Full Path to Full-Path Indexing (5)

**Concur** — The Bw-Tree: A B-tree for New Hardware Pla (5); BzTree: A High-Performance Latch-free Rang (5); The Bw-Tree: A Latch-Free B-Tree for Log-S (5); NBTree: a Lock-free PM-friendly Persistent (5); PACTree: A High Performance Persistent Ran (5)

**Cache** — The Bw-Tree: A B-tree for New Hardware Pla (5); The Bw-Tree: A Latch-Free B-Tree for Log-S (5); DFTL: A Flash Translation Layer Employing  (5); Building a Bw-Tree Takes More Than Just Bu (5); Bridging Cache-Friendliness and Concurrenc (5)

**Scan** — The Bw-Tree: A B-tree for New Hardware Pla (5); BzTree: A High-Performance Latch-free Rang (5); PACTree: A High Performance Persistent Ran (5); Evaluating Persistent Memory Range Indexes (5); bcachefs: Principles of Operation (5)

**Snap** — B-trees, Shadowing, and Clones (5); Btrfs: The Swiss Army Knife of Storage (5); The Zettabyte File System (5); File System Design for an NFS File Server  (5); APFS Internals for Forensic Analysis (ERNW (5)
