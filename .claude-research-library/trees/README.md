# Tree-family data structures — research library

Papers on **concurrent / lock-free B-trees, B+trees, and the algorithms they are
built from** — gathered for a planned **lock-free B+tree** in
`portable-collections`. Because the workspace is `no_std` + **`unsafe`-free**, the
central question these papers answer is not just *which* lock-free design, but
*how* (and whether) it can be re-expressed safely.

Downloaded 2026-06-21 from arXiv, open conference proceedings (PVLDB, USENIX,
IEEE), and author / institution pages (all freely available; no paywall
circumvention). arXiv items carry the arXiv id as the filename suffix; others
carry `_Author_Year`. `★` marks a paper centrally about a **lock-free / latch-free
B-tree itself** (vs a supporting technique). Each entry: filename — *title*
(authors; venue, year) — one-line relevance to building a lock-free B+tree. **Section I** (added in a
second pass) extends the library toward **filesystem** use — on-disk crash
consistency, write-optimization, and log-structured storage. A companion file,
[`fs-suitability-scores.md`](fs-suitability-scores.md), scores every paper 0–5
across seven filesystem use-case dimensions (metadata index, crash consistency,
write-optimization, concurrency, DRAM cache, range scan, snapshots).

---

## A. Lock-free / latch-free B+trees — the core designs

The papers most directly on point: a B-tree/B+tree whose concurrent insert / delete / split / merge are done **without locks** (in-place CAS, delta records + mapping table, multi-word CAS, or bulk-synchronous batching).

- `a-lock-free-bplustree_Braginsky-Petrank_2012.pdf` — ★ *A Lock-Free B+tree* (Braginsky, Petrank; SPAA 2012 (ACM Symposium on Parallelism in Algorithms and Architectures), 2012). The canonical paper: first lock-free, dynamic, balanced B+tree using only standard CAS. Direct blueprint for CAS-based concurrent insert/delete/search in a B+tree.
- `the-bw-tree-a-b-tree-for-new-hardware-platforms_Levandoski-Lomet-Sengupta_2013.pdf` — ★ *The Bw-Tree: A B-tree for New Hardware Platforms* (Levandoski, Lomet, Sengupta; ICDE 2013 (IEEE International Conference on Data Engineering), 2013). Latch-free B-tree via delta records + an indirection mapping table updated by atomic CAS; the most influential latch-free B-tree design and a key alternative to in-place CAS.
- `the-bw-tree-a-latch-free-b-tree-for-log_Levandoski-Lomet-Sengupta_2013.pdf` — ★ *The Bw-Tree: A Latch-Free B-Tree for Log-Structured Flash Storage* (Levandoski, Lomet, Sengupta; IEEE Data Engineering Bulletin 36(2) (companion to ICDE 2013), 2013). Canonical latch-free B-tree: a mapping table indirection plus prepended delta records updated by single-word CAS, with epoch-based reclamation. Core design template for a CAS-only B+tree.
- `building-a-bw-tree-takes-more-than-just-buzz-words_Wang-Pavlo-Lin_2018.pdf` — ★ *Building a Bw-Tree Takes More Than Just Buzz Words* (Wang, Pavlo, Lin, et al.; SIGMOD 2018 (ACM International Conference on Management of Data), 2018). OpenBw-Tree: fills in the Bw-tree's undocumented engineering details (epoch reclamation, consolidation, mapping table) needed to actually implement a correct latch-free B-tree. Essential companion to the original.
- `palm-parallel-architecture-friendly-latch-free-modifications-to-bplus-trees_Sewall-Chhugani-Kim-etal_2011.pdf` — ★ *PALM: Parallel Architecture-Friendly Latch-Free Modifications to B+ Trees on Many-Core Processors* (Sewall, Chhugani, Kim, Satish, Dubey; PVLDB Vol. 4 / VLDB 2011, 2011). Latch-free B+tree via Bulk Synchronous Parallel batching: groups queries into atomic stages that preclude contention, plus SIMD in-node search. A different (batched, deadlock-free) route to a latch-free B+tree.
- `bztree-a-high-performance-latch-free-range-index-for-non_Arulraj-Levandoski-Minhas-etal_2018.pdf` — ★ *BzTree: A High-Performance Latch-free Range Index for Non-Volatile Memory* (Arulraj, Levandoski, Minhas, Larson; PVLDB Vol. 11 No. 5 / VLDB 2018, 2018). Latch-free B-tree built entirely on a (persistent) multi-word CAS (PMwCAS) primitive, simplifying the concurrency logic vs Bw-tree. Shows how MwCAS makes lock-free B-tree splits/merges tractable; relevant even ignoring NVM.
- `elb-trees-an-efficient-and-lock-free-b-tree-derivative_1308.6145.pdf` — ★ *ELB-Trees, An Efficient and Lock-free B-tree Derivative* (Bonnichsen, Karlsson, Probst; arXiv (extended MULTIPROG/MUCOCOS'13 report), 2013). An explicitly lock-free B-tree variant (CAS-based, no locks) targeting scalable concurrent search structures; a concrete alternative lock-free B-tree design point to compare against the Bw-tree.
- `fbplus-tree-a-memory-optimized-bplus-tree-with-latch-free_2503.23397.pdf` — ★ *FB+-tree: A Memory-Optimized B+-tree with Latch-Free Update* (Chen, Li, Li, Deng; PVLDB / VLDB 2025 (also arXiv), 2025). Recent main-memory B+tree with latch-free updates: combines the B-link technique with optimistic locking and subtle atomic ops for latch-free writes plus concurrent lookups. State-of-the-art design to study.

## B. Concurrent B-tree synchronization techniques — the locking schemes a lock-free design must beat

Not lock-free, but the canonical concurrency protocols for B-trees; the baselines and the source of ideas (link pointers, version validation, optimistic coupling) reused by the lock-free designs above.

- `efficient-locking-for-concurrent-operations-on-b-trees_Lehman-Yao_1981.pdf` — *Efficient Locking for Concurrent Operations on B-Trees* (Lehman, Yao; ACM Transactions on Database Systems (TODS) 6(4), 1981). Introduces the B-link tree (right-sibling links + high keys) that lets a search proceed without locking and decouples node splits from parent updates; the structural foundation every lock-free B+tree builds on.
- `cache-conscious-concurrency-control-of-main-memory-indexes-on-shared_Cha-Hwang-Kim-etal_2001.pdf` — *Cache-Conscious Concurrency Control of Main-Memory Indexes on Shared-Memory Multiprocessor Systems* (Cha, Hwang, Kim, Kwon; VLDB 2001, 2001). Optimistic Latch-Free Index Traversal: version-counter based read/update primitives eliminate latching on the read path of a B+tree. Foundational technique (precursor to optimistic lock coupling) for any near-lock-free B+tree.
- `the-art-of-practical-synchronization_Leis-Scheibner-Kemper-etal_2016.pdf` — *The ART of Practical Synchronization* (Leis, Scheibner, Kemper, Neumann; DaMoN 2016 (workshop, SIGMOD), 2016). Defines Optimistic Lock Coupling (version-counter validation) and ROWEX, the two synchronization protocols most practical for in-memory ordered indexes and a pragmatic near-lock-free alternative to full Bw-tree complexity.
- `elimination-a-b-trees-with-fast-durable-updates_2112.15259.pdf` — *Elimination (a,b)-trees with fast, durable updates* (Srivastava, Brown; PPoPP 2022 (also arXiv), 2022). Concurrent (a,b)-tree (generalization of a B-tree); OCC-ABtree/Elim-ABtree use optimistic concurrency + operation elimination for update-heavy workloads. Strong, recent, open-access reference for high-throughput concurrent B-tree updates.

## C. Persistent / NVM lock-free B+trees & their primitives

Lock-free B+trees for byte-addressable persistent memory, plus the persistent multi-word-CAS and flush/fence primitives they rely on. (NVM is out of this workspace's current scope, but the *concurrency* mechanics transfer.)

- `nbtree-a-lock-free-pm-friendly-persistent-bplus-tree-for_Zhang-Hua-Liu_2022.pdf` — ★ *NBTree: a Lock-free PM-friendly Persistent B+-Tree for eADR-enabled PM Systems* (Zhang, Hua, Liu, et al.; PVLDB Vol. 15 (VLDB 2022), 2022). A fully lock-free persistent B+tree exploiting eADR (persistent CPU cache): log-structured leaf inserts + in-place update/delete via CAS, shift-aware reads. The most recent end-to-end lock-free persistent B+tree design to model.
- `nv-tree-a-consistent-and-workload-adaptive-tree-structure-for_Yang-Wei-Chen-etal_2015.pdf` — ★ *NV-Tree: A Consistent and Workload-adaptive Tree Structure for Non-volatile Memory* (Yang, Wei, Chen, Wang, He, Bhuyan; USENIX FAST 2015 (extended in IEEE TC), 2015). Early persistent B+tree that slashes cache-line flushes (~96%) by keeping leaf entries append-only/unsorted and reconstructable inner nodes — key consistency-cost ideas any lock-free persistent B+tree must address.
- `endurable-transient-inconsistency-in-byte-addressable-persistent-bplus-tree-fast_Hwang-Kim-Won-etal_2018.pdf` — ★ *Endurable Transient Inconsistency in Byte-Addressable Persistent B+-Tree (FAST and FAIR)* (Hwang, Kim, Won, Nam; USENIX FAST 2018, 2018). Failure-Atomic Shift / In-place Rebalance make every 8-byte store leave the tree consistent or read-tolerable, enabling lock-free (latch-free) reads with no copy-on-write or logging. Core technique for non-blocking reads in a persistent B+tree.
- `pactree-a-high-performance-persistent-range-index-using-pac-guidelines_Kim-Hwang-Kim-etal_2021.pdf` — *PACTree: A High Performance Persistent Range Index Using PAC Guidelines* (Kim, Hwang, Kim, Kwon, Koh, Nguyen; ACM SOSP 2021, 2021). Concurrent persistent range index (trie of B+tree-like leaves) with asynchronous SMOs and ROWEX-style optimistic concurrency. Useful for the concurrency-control + structural-modification side of a lock-free/low-contention persistent B+tree.
- `easy-lock-free-indexing-in-non-volatile-memory_Wang-Levandoski-Larson_2018.pdf` — *Easy Lock-Free Indexing in Non-Volatile Memory* (Wang, Levandoski, Larson; ICDE 2018, 2018). Defines PMwCAS (persistent multi-word CAS), the durable lock-free primitive BzTree is built on. Lets you atomically swap several 8-byte words across NVM with built-in recovery — the core building block for a lock-free B+tree's node splits/merges.
- `practical-persistent-multi-word-compare-and-swap-algorithms-for-many_2404.01710.pdf` — *Practical Persistent Multi-Word Compare-and-Swap Algorithms for Many-Core CPUs* (Sugiura, Nishimura, Ishikawa; arXiv (cs.DB); IPSJ JIP 2024, 2024). Modern, faster PMwCAS: removes redundant CAS/flush ops and dirty flags (descriptors double as write-ahead logs), up to ~10x faster on many-core CPUs. Directly relevant if you implement PMwCAS-style atomics for a lock-free B+tree.
- `persistent-memory-i-o-primitives_1904.01614.pdf` — *Persistent Memory I/O Primitives* (van Renen, Vogel, Leis, Neumann, Kemper; arXiv / DaMoN 2019 (journal: VLDB J. 29(6) 2020), 2019). First comprehensive measurement of real persistent-memory bandwidth/latency plus reusable low-level primitives (log writing, block flushing, in-place updates, latency-hiding coroutines). The open arXiv precursor to the paywalled VLDB Journal version; foundational if the B+tree ever targets NVM/persistent backends.

## D. Non-blocking ordered trees & alternatives — the algorithmic foundations

Lock-free BSTs, tries, and skip lists. A lock-free B+tree borrows their machinery (LLX/SCX, marking, cooperative helping) and competes with them as an ordered map; the skip list is the usual lock-free *alternative* to a B+tree.

- `non-blocking-binary-search-trees_Ellen-Fatourou-Ruppert-etal_2010.pdf` — *Non-blocking Binary Search Trees* (Ellen, Fatourou, Ruppert, van Breugel; PODC 2010 (full version, author-hosted), 2010). The foundational lock-free unbalanced BST (CAS-only, child-pointer marking via Info records); the algorithmic template every later lock-free ordered tree, including a B+tree, extends.
- `pragmatic-primitives-for-non-blocking-data-structures_1712.06688.pdf` — *Pragmatic Primitives for Non-blocking Data Structures* (Brown, Ellen, Ruppert; PODC 2013 (arXiv 2017), 2013). Defines LLX/SCX/VLX, multi-word LL/SC/VL built from single-word CAS, that let an update atomically modify a contiguous group of nodes; the synchronization substrate for atomically splitting/merging B+tree nodes.
- `a-general-technique-for-non-blocking-trees_1712.06687.pdf` — *A General Technique for Non-blocking Trees* (Brown, Ellen, Ruppert; PPoPP 2014 (arXiv 2017), 2014). Generic recipe for provably-correct lock-free down-pointer trees using LLX/SCX, demonstrated on a chromatic (relaxed red-black) tree; the closest published methodology for deriving a correct lock-free balanced/B-like tree.
- `non-blocking-k-ary-search-trees_Brown-Helga_2011.pdf` — *Non-blocking k-ary Search Trees* (Brown, Helga; OPODIS 2011 (author-hosted), 2011). Generalizes Ellen et al. to internal nodes with k children, the exact step from binary toward high-fanout B-tree-style nodes; quantifies the fanout-vs-update-contention tradeoff central to a lock-free B+tree.
- `efficient-lock-free-binary-search-trees_1404.3272.pdf` — *Efficient Lock-free Binary Search Trees* (Chatterjee, Nguyen, Tsigas; PODC 2014 (arXiv), 2014). Lock-free internal BST with O(H(n)+c) amortized step complexity and contention-adaptive helping; improved disjoint-access-parallelism, the property a scalable lock-free B+tree must preserve across node updates.
- `persistent-non-blocking-binary-search-trees-supporting-wait-free-range_1805.04779.pdf` — *Persistent Non-Blocking Binary Search Trees Supporting Wait-Free Range Queries* (Fatourou, Ruppert; SPAA 2019 (arXiv), 2018). Adds wait-free linearizable range queries to the Ellen et al. lock-free BST via lightweight local helping; range scans are exactly the ordered-iteration feature that motivates choosing a B+tree over a hash index.
- `a-lock-free-binary-trie_2405.06208.pdf` — *A Lock-free Binary Trie* (Jeremy Ko; arXiv (cs.DC), 2024). Lock-free trie supporting ordered (predecessor/successor) operations with amortized analysis. Trie/radix-style ordered structure is a strong alternative backend and a source of techniques for CAS-based ordered concurrent indexing relevant to a lock-free B+tree's leaf chaining.
- `concurrent-balanced-augmented-trees_2601.05225.pdf` — *Concurrent Balanced Augmented Trees* (Wrench, Singh, Roh, Fatourou, Jayanti, Ruppert, Wei; PPoPP 2026 / arXiv, 2026). First lock-free balanced augmented search tree with generic augmentation, supporting aggregation/order-statistic/range queries plus standard ops; uses delegation for scalability. Shows how to keep a balanced ordered tree lock-free and memory-safe, the core challenge for a lock-free B+tree.
- `a-provably-correct-scalable-concurrent-skip-list_Herlihy-Lev-Luchangco-etal_2006.pdf` — *A Provably Correct Scalable Concurrent Skip List* (Herlihy, Lev, Luchangco, Shavit; OPODIS 2006 (author-hosted), 2006). The canonical scalable concurrent skip list (lock-free search, brief lock validation, logical-before-physical delete): the main alternative ordered structure to benchmark a lock-free B+tree against.
- `bridging-cache-friendliness-and-concurrency-a-locality-optimized-in-memory_2507.21492.pdf` — *Bridging Cache-Friendliness and Concurrency: A Locality-Optimized In-Memory B-Skiplist* (Luo, Hao, Wheatman, Pandey, Xu; ICPP 2025 (arXiv), 2025). B-skiplist hybrid with a single-pass top-down insert and matching top-down concurrency control; bridges skip-list simplicity and B-tree cache-locality, a useful design and benchmark target for an ordered lock-free index.

## E. Safe memory reclamation — the linchpin of any lock-free tree in a non-GC language

**The hardest part of porting any of the above to Rust.** Without a GC, a node unlinked by one thread may still be read by another; these are the schemes (epochs, hazard pointers and successors) that make freeing safe. crossbeam-epoch implements the EBR line.

- `hazard-pointers-safe-memory-reclamation-for-lock-free-objects_Michael_2004.pdf` — *Hazard Pointers: Safe Memory Reclamation for Lock-Free Objects* (Maged M. Michael; IEEE Transactions on Parallel and Distributed Systems (TPDS) 15(6), 2004). The foundational hazard-pointer scheme: per-thread published pointers that block reclamation; the baseline robust (bounded-garbage) reclamation every lock-free B+tree must consider for safe node freeing.
- `practical-lock-freedom-ucam-cl-tr-579_Fraser_2004.pdf` — *Practical Lock-Freedom (UCAM-CL-TR-579)* (Keir Fraser; University of Cambridge Computer Laboratory Technical Report (PhD thesis), 2004). Origin of epoch-based reclamation (EBR): the global-epoch / grace-period design that crossbeam-epoch descends from. Also presents lock-free search trees, directly informing a lock-free B+tree.
- `reclaiming-memory-for-lock-free-data-structures-there-has-to_1712.01044.pdf` — *Reclaiming Memory for Lock-Free Data Structures: There has to be a Better Way* (Trevor Brown; PODC 2015 (arXiv 2017), 2015). DEBRA: a distributed, fault-tolerant EBR with neutralizing signals and a clean reclamation abstraction; the practical EBR variant most lock-free tree implementations (incl. B-tree-like) actually use.
- `interval-based-memory-reclamation_Wen-Izraelevitz-Cai-etal_2018.pdf` — *Interval-Based Memory Reclamation* (Wen, Izraelevitz, Cai, Beadle, Scott; PPoPP 2018, 2018). IBR compares a thread's reserved epoch interval to each block's birth/retire lifetime, getting hazard-pointer-like bounded memory with EBR-like speed; strong fit for many-node tree traversals.
- `brief-announcement-hazard-eras-non-blocking-memory-reclamation_Ramalhete-Correia_2017.pdf` — *Brief Announcement: Hazard Eras - Non-Blocking Memory Reclamation* (Pedro Ramalhete, Andreia Correia; SPAA 2017, 2017). Hazard Eras: drop-in hazard-pointer API but threads reserve an era (epoch) instead of a pointer, combining HP robustness with EBR-level throughput; well-suited to optimistic B+tree node access.
- `nbr-neutralization-based-reclamation_2012.14542.pdf` — *NBR: Neutralization Based Reclamation* (Ajay Singh, Trevor Brown, Ali Mashtizadeh; PPoPP 2021, 2021). NBR uses OS signals to neutralize/restart read-only operations so retired nodes can be freed; often beats EBR and HP with bounded memory and is easy to apply to tree-shaped structures.
- `applying-hazard-pointers-to-more-concurrent-data-structures-hpplusplus_Jung-Lee-Kim-etal_2023.pdf` — *Applying Hazard Pointers to More Concurrent Data Structures (HP++)* (Jaehwang Jung, Janggun Lee, Jeonghyeon Kim, Jeehoon Kang; SPAA 2023, 2023). HP++ extends hazard pointers to support optimistic traversal (under-approximate unreachability + patch-up), enabling HP-style robustness on data structures with optimistic reads like a latch-free B+tree.
- `crystalline-fast-and-memory-efficient-wait-free-reclamation_2108.02763.pdf` — *Crystalline: Fast and Memory Efficient Wait-Free Reclamation* (Ruslan Nikolaev, Binoy Ravindran; DISC 2021 (arXiv), 2021). Wait-free reclamation that is simultaneously fast and memory-efficient (improving on Hyaline/WFE); a strong candidate if the B+tree needs wait-free progress guarantees for reclamation.
- `are-your-epochs-too-epic-batch-free-can-be-harmful_2401.11347.pdf` — *Are Your Epochs Too Epic? Batch Free Can Be Harmful* (Daewoo Kim, Trevor Brown, Ajay Singh; PPoPP 2024 (arXiv), 2024). Shows EBR's large free-batches fight modern allocator thread-caches and proposes amortized freeing; a critical practical-tuning lesson, evaluated on a lock-free AB-tree (close to a B+tree).
- `publish-on-ping-a-better-way-to-publish-reservations-in_2501.04250.pdf` — *Publish on Ping: A Better Way to Publish Reservations in Memory Reclamation for Concurrent Data Structures* (Ajay Singh, Trevor Brown; PPoPP 2025 (arXiv), 2025). Recent (2025) technique removing the per-read fence/announce cost of hazard-pointer-style reservations via POSIX-signal publish-on-demand; directly attacks the traversal overhead a deep B+tree pays.
- `making-lockless-synchronization-fast-performance-implications-of-memory-reclamation_Hart-McKenney-Brown-etal_2006.pdf` — *Making Lockless Synchronization Fast: Performance Implications of Memory Reclamation* (Hart, McKenney, Brown, Walpole; IPDPS 2006 (extended in JPDC 2007), 2006). The foundational fair comparison of quiescent-state, epoch-based, and hazard-pointer reclamation under a flexible microbenchmark. Conclusion (no globally optimal scheme; choice depends on data structure, workload, environment) is the starting point for picking a reclamation strategy for a lock-free B+tree's freed nodes.
- `a-new-and-five-older-concurrent-memory-reclamation-schemes-in_1712.06134.pdf` — *A new and five older Concurrent Memory Reclamation Schemes in Comparison (Stamp-it)* (Pöter, Träff; arXiv (cs.DC), 2017). Modern, large-scale benchmark (48-512 hardware threads) comparing six reclamation schemes: lock-free reference counting, hazard pointers, QSBR, EBR, NEBR, and Stamp-it. Updates the Hart-era picture with high-core-count data critical for choosing reclamation in a scalable lock-free B+tree.

## F. Modern / learned / specialized concurrent indexes

Recent (2023-2025) takes on concurrent ordered indexing: data-parallel B-trees, STM-backed maps, lock-free learned indexes, and hardware (CXL) considerations — the current frontier to benchmark against.

- `bs-tree-a-gapped-data-parallel-b-tree_2505.01180.pdf` — *BS-tree: A gapped data-parallel B-tree* (Tsitsigkos, Michalopoulos, Mamoulis, Terrovitis; arXiv (cs.DB); to appear ICDE 2026, 2025). In-memory B+tree with gap slots + duplicated keys enabling branchless SIMD node search and shift-free updates, with FOR compression; relevant for cache/SIMD-friendly node layout in a high-fanout concurrent B+tree (single- and multi-threaded evaluation).
- `skip-hash-a-fast-ordered-map-via-software-transactional-memory_2410.07466.pdf` — *Skip Hash: A Fast Ordered Map Via Software Transactional Memory* (Rodriguez, Aksenov, Spear; arXiv (cs.DC), 2024). Fast concurrent ordered map combining skip list + hash map, using STM for cheap multi-word atomic updates and a range-query manager giving linearizable range scans without sacrificing insert/remove throughput. Directly informs lock-free ordered-map range-query design.
- `fresh-a-lock-free-data-series-index_2310.11602.pdf` — *FreSh: A Lock-Free Data Series Index* (Fatourou, Kosmas, Palpanas, Paterakis; SRDS 2023 / arXiv, 2023). Presents 'Refresh', a generic technique to make a locality-aware index lock-free while matching blocking performance, plus a framework for modular design/analysis of concurrent indexes. The generic lock-free transformation and analysis method transfer to a lock-free B+tree.
- `learned-lock-free-search-data-structures_2308.11205.pdf` — *Learned Lock-free Search Data Structures* (Bhardwaj, Chatterjee, Sharma, Peri, Nayak; arXiv (cs.DC), 2023). Proposes Kanva: a linearizable non-blocking learned index using a shallow hierarchy of linear models over lock-free bins, beating non-blocking interpolation/(a,b)-trees. Shows how to fuse learned models with lock-free ordered search, an option for a learned-accelerated lock-free B+tree.
- `sali-a-scalable-adaptive-learned-index-framework-based-on-probability_2308.15012.pdf` — *SALI: A Scalable Adaptive Learned Index Framework based on Probability Models* (Ge, H. Zhang, Shi, Luo, Guo, Chai, Chen, Pan; SIGMOD 2024 / arXiv, 2023). Scalable concurrent updatable learned index: node-evolving strategies plus lightweight statistics to handle workload skew and many-thread insert scalability (2.04x at 64 threads over LIPP). Reference for concurrency control and scalability in modern ordered indexes.
- `guidelines-for-building-indexes-on-partially-cache-coherent-cxl-shared_2511.06460.pdf` — *Guidelines for Building Indexes on Partially Cache-Coherent CXL Shared Memory* (Wu, Dong, Cai, Yan, Chen; arXiv (cs.OS), 2025). Engineering study deriving SP and P3 guidelines (out-of-place updates, replicated shared variables, speculative reads) to make cache-coherent concurrent indexes (incl. BwTree) correct and fast on relaxed-coherence CXL hardware, up to 16x faster. Concretely catalogs concurrency-control pitfalls a portable lock-free B+tree must address.

## G. Surveys & benchmarks — how to evaluate the result

Experimental comparisons and benchmarking pitfalls; read before designing the performance study for a new lock-free B+tree.

- `evaluating-persistent-memory-range-indexes_Lersch-Hao-Oukid-etal_2019.pdf` — *Evaluating Persistent Memory Range Indexes* (Lersch, Hao, Oukid, Wang, Willhalm; PVLDB 13(4), 2019). Apples-to-apples experimental comparison of B+tree-family range indexes (wBTree, NV-Tree, BzTree, FPTree) including concurrency and recovery. The benchmark methodology (PiBench) and the multi-threaded scaling results anchor how to evaluate an ordered-map index landscape, lock-free B+trees included.
- `evaluating-persistent-memory-range-indexes-part-two-extended-version_2201.13047.pdf` — *Evaluating Persistent Memory Range Indexes: Part Two [Extended Version]* (He, Lu, Huang, Wang; arXiv (cs.DB); PVLDB, 2022). Apples-to-apples evaluation of state-of-the-art PM range indexes (incl. BzTree, FAST&FAIR, DPTree, PACTree-class) on real Optane. Best single source for choosing a lock-free persistent B+tree backend and avoiding known pitfalls.
- `performance-anomalies-in-concurrent-data-structure-microbenchmarks_2208.08469.pdf` — *Performance Anomalies in Concurrent Data Structure Microbenchmarks* (Kharal, Brown; OPODIS 2022, 2022). Shows that concurrent-data-structure benchmark results can swing 10-100x and even invert rankings depending on microbenchmark design choices. A methodological guardrail for honestly benchmarking a lock-free B+tree against BTreeMap and rivals.

## H. Formal verification / correctness — proving it linearizable

Techniques and templates for machine-checked correctness of concurrent / lock-free search structures — the safety net for a tricky CAS-heavy design.

- `verifying-concurrent-search-structure-templates_Krishna-Patel-Shasha-etal_2020.pdf` — *Verifying Concurrent Search Structure Templates* (Krishna, Patel, Shasha, Wies; PLDI 2020, 2020). Mechanizes (Iris/Coq) the link, give-up, and lock-coupling search-structure templates and derives a verified B-tree (B-link) implementation - the canonical machine-checked proof recipe for a concurrent/latch-free B+tree, decoupling thread-safety from structural integrity.
- `verifying-lock-free-search-structure-templates_2405.13271.pdf` — *Verifying Lock-free Search Structure Templates* (Patel, Shasha, Wies; ECOOP 2024 (arXiv extended version), 2024). Fully mechanized Iris linearizability proofs of lock-free search-structure templates (lists/skiplists) with future-dependent linearization points via hindsight/prophecy reasoning - the proof techniques you must reuse when arguing a lock-free B+tree's unsynchronized traversals are linearizable.
- `proving-highly-concurrent-traversals-correct_2010.00911.pdf` — *Proving Highly-Concurrent Traversals Correct* (Feldman, Khyzha, Enea, Morrison, Nanevski, Rinetzky, Shoham; OOPSLA 2020 (PACMPL), 2020). General technique for proving linearizability of unsynchronized (lock-free) tree traversals using only sequential properties plus a simple write-mutation condition - directly applicable to the hardest part of a lock-free B+tree: arguing read-only root-to-leaf descents see a consistent snapshot.
- `verifying-concurrent-multicopy-search-structures_2109.05631.pdf` — *Verifying Concurrent Multicopy Search Structures* (Patel, Krishna, Shasha, Wies; OOPSLA 2021 (arXiv extended version), 2021). Iris framework for linearizability of multicopy (LSM/differential) search structures abstracting from in-memory layout, enabling proof reuse across implementations - the methodology for verifying delta/append-style indexes that a Bw-tree-like lock-free B+tree resembles.
- `verifying-linearizability-a-comparative-survey_1410.6268.pdf` — *Verifying Linearizability: A Comparative Survey* (Dongol, Derrick; ACM Computing Surveys (arXiv preprint), 2014). Survey mapping the landscape of linearizability-verification techniques (refinement, shape analysis, reduction, rely-guarantee) with unified terminology - the orientation map for choosing a proof method for a lock-free B+tree.

## I. Filesystem trees — on-disk crash consistency, write-optimization & log-structured storage

Added 2026-06-21 (second pass). A file-system-suitability read of sections A-H
showed this library was skewed toward *in-memory* and *byte-addressable-NVM*
designs (its only block-device/external-memory item is the Lehman-Yao B-link tree
in B), and under-covered the two structure families most central to a real
**on-disk** filesystem: **copy-on-write / shadow-paging B-trees** (the
btrfs/ZFS/APFS crash-consistency backbone) and **write-optimized external-memory
trees** (Bε-tree / LSM). This section fills that gap. Generic write-optimization
roots — the Bε-tree intro (Bender 2015), the LSM original (O'Neil 1996), and
COLA / streaming B-trees — already live in the sibling library
`~/research-library/data-structures/` and are deliberately **not** duplicated here.

### I-1. Copy-on-write / shadow-paging B-trees — the crash-consistency backbone

How btrfs / ZFS / APFS / bcachefs survive crashes: never overwrite in place; write new tree nodes to free space and atomically flip the root. The on-disk durability model this whole library was missing.

- `b-trees-shadowing-and-clones_Rodeh_2008.pdf` — *B-trees, Shadowing, and Clones* (Ohad Rodeh; ACM Transactions on Storage (TOS), Vol. 3, No. 4 (also Linux Symposium 2007), 2008). The foundational algorithm: how to make a B-tree respect copy-on-write/shadowing while keeping concurrency and supporting writable clones (snapshots). This is the exact design btrfs's tree is built on and the canonical reference for a CoW index inside a filesystem.
- `btrfs-the-swiss-army-knife-of-storage_Bacik_2012.pdf` — *Btrfs: The Swiss Army Knife of Storage* (Josef Bacik; USENIX ;login: Vol. 37, No. 6, 2012). Open-access companion to the btrfs TOS paper by btrfs's lead developer: explains the CoW B-tree, snapshots/clones, and how subvolumes map onto the shadowed-tree design. Stands in for the paywalled TOS 2013 article as a reliably downloadable filesystem-specific source.
- `the-zettabyte-file-system_Bonwick-Ahrens-Henson-etal_2003.pdf` — *The Zettabyte File System* (Jeff Bonwick, Matt Ahrens, Val Henson, Mark Maybee, Mark Shellenbaum; USENIX FAST 2003 (work-in-progress report), Sun Microsystems, 2003). The original ZFS paper: pooled storage with a transactional copy-on-write model and self-validating Merkle-checksummed block trees. The other pillar (with WAFL) of shadow-paging filesystem design that motivated CoW-friendly tree structures.
- `file-system-design-for-an-nfs-file-server-appliance_Hitz-Lau-Malcolm_1994.pdf` — *File System Design for an NFS File Server Appliance* (Dave Hitz, James Lau, Michael Malcolm; USENIX Winter 1994 Technical Conference (NetApp Technical Report TR3002), 1994). WAFL: the original Write-Anywhere File Layout that pioneered shadow-paging/copy-on-write snapshots in a production filesystem via a tree of block pointers rooted at the superblock. The historical root of the shadow-paging backbone that btrfs/ZFS later generalized.
- `apfs-internals-for-forensic-analysis-ernw-whitepaper-65_Dewald-Plum_2018.pdf` — *APFS Internals for Forensic Analysis (ERNW Whitepaper 65)* (Andreas Dewald, Jonas Plum; ERNW Whitepaper 65 (Public), 2018). Most thorough openly available technical description of APFS internals: its B-tree object structure, object map (omap) B-tree, checkpoints, and copy-on-write cloning. Reverse-engineered reference for Apple's CoW B-tree filesystem (the 'Decoding the APFS file system' academic paper is paywalled on Elsevier).
- `bcachefs-principles-of-operation_Overstreet_2026.pdf` — *bcachefs: Principles of Operation* (Kent Overstreet; bcachefs.org (official design document, author-hosted), 2026). Design of bcachefs: a CoW filesystem whose B-trees use large (256 KiB) nodes that are internally log-structured (append-only update vectors), a hybrid that cuts node-rewrite write amplification while keeping ordered B-tree semantics and snapshots. Directly relevant to choosing a write-friendly node layout for a tree index.

### I-2. Write-optimized (Bε-tree) file systems — the BetrFS line

Buffer updates high in the tree and flush them down in batches: turns many small random metadata writes into few large sequential ones (write-amplification ↓), while keeping B-tree-class point/range queries. The filesystem realization of the Bε-tree (generic Bε-tree intro lives in the sibling library).

- `betrfs-a-right-optimized-write-optimized-file-system_Jannen-Yuan-Zhan-etal_2015.pdf` — *BetrFS: A Right-Optimized Write-Optimized File System* (William Jannen, Jun Yuan, Yang Zhan, Amogh Akshintala, John Esmet, Yizheng Jiao, Ankur Mittal, Prashant Pandey, Phaneendra Reddy, Leif Walsh, Michael Bender, Martin Farach-Colton, Rob Johnson, Bradley C. Kuszmaul, Donald E. Porter; USENIX FAST 2015 (13th USENIX Conference on File and Storage Technologies), 2015). Foundational BetrFS paper: first in-kernel file system built on a write-optimized index (Be-tree / fractal tree), mapping the tree/index structure directly onto FS metadata and small-write paths.
- `optimizing-every-operation-in-a-write-optimized-file-system_Yuan-Zhan-Jannen-etal_2016.pdf` — *Optimizing Every Operation in a Write-optimized File System* (Jun Yuan, Yang Zhan, William Jannen, Prashant Pandey, Amogh Akshintala, Kanchan Chandnani, Pooja Deo, Zardosht Kasheff, Leif Walsh, Michael Bender, Martin Farach-Colton, Rob Johnson, Bradley C. Kuszmaul, Donald E. Porter; USENIX FAST 2016 (14th USENIX Conference on File and Storage Technologies) — Best Paper, 2016). Removes the write-optimization trade-off via late-binding journaling, zoning, and range deletion inside the Be-tree FS — directly about crash consistency (journaling) and write amplification of the tree-backed FS.
- `the-full-path-to-full-path-indexing_Zhan-Conway-Jiao-etal_2018.pdf` — *The Full Path to Full-Path Indexing* (Yang Zhan, Alex Conway, Yizheng Jiao, Eric Knorr, Michael A. Bender, Martin Farach-Colton, William Jannen, Rob Johnson, Donald E. Porter, Jun Yuan; USENIX FAST 2018 (16th USENIX Conference on File and Storage Technologies), 2018). Shows how to key the Be-tree FS index by full pathname (vs inode-based) to get fast scans/writes/renames — core question of how the on-disk index/tree is keyed and laid out for locality.
- `how-to-copy-files_Zhan-Conway-Jiao-etal_2020.pdf` — *How to Copy Files* (Yang Zhan, Alexander Conway, Yizheng Jiao, Nirjhar Mukherjee, Ian Groombridge, Michael A. Bender, Martin Farach-Colton, William Jannen, Rob Johnson, Donald E. Porter, Jun Yuan; USENIX FAST 2020 (18th USENIX Conference on File and Storage Technologies), 2020). Nimble clones (copy-on-abundant-write) in the full-path-indexed Be-tree FS: how to share/copy subtrees of the index while preserving locality — a tree-structure-level copy mechanism.
- `the-tokufs-streaming-file-system_Esmet-Bender-Farach-Colton-etal_2012.pdf` — *The TokuFS Streaming File System* (John Esmet, Michael A. Bender, Martin Farach-Colton, Bradley C. Kuszmaul; USENIX HotStorage 2012 (4th USENIX Workshop on Hot Topics in Storage and File Systems), 2012). Direct predecessor of BetrFS: a Fractal-tree-indexed userspace file system, showing the tree-as-FS-index design on microdata write/read workloads before the in-kernel BetrFS.
- `betrfs-write-optimization-in-a-kernel-file-system_Jannen-Yuan-Zhan-etal_2015.pdf` — *BetrFS: Write-Optimization in a Kernel File System* (William Jannen, Jun Yuan, Yang Zhan, Amogh Akshintala, John Esmet, Yizheng Jiao, Ankur Mittal, Prashant Pandey, Phaneendra Reddy, Leif Walsh, Michael Bender, Martin Farach-Colton, Rob Johnson, Bradley C. Kuszmaul, Donald E. Porter; ACM Transactions on Storage (TOS), Vol. 11, No. 4, Article 18, 2015). Journal-length treatment of the BetrFS design (Be-tree/WODS as the kernel FS index), with fuller detail on the data-structure integration than the FAST 2015 conference paper; ACM DL is paywalled so this is the author-hosted open PDF.

### I-3. Log-structured, flash & zoned file systems

The sequential-write / append-only storage context a filesystem tree sits in: log-structuring (LFS), flash-native layout (F2FS), zoned devices (ZNS), and the FTL underneath. Sets the write-amplification and device-endurance constraints.

- `the-design-and-implementation-of-a-log-structured-file-system_Rosenblum-Ousterhout_1992.pdf` — *The Design and Implementation of a Log-Structured File System* (Mendel Rosenblum, John K. Ousterhout; ACM Transactions on Computer Systems (TOCS) 10(1) (orig. SOSP 1991), 1992). The foundational sequential-write FS design: turns all writes into a single append-only log, motivating the write-amplification / GC context every FS index structure must live within; its inode-map indirection is the original on-disk index-relocation problem.
- `f2fs-a-new-file-system-for-flash-storage_Lee-Sim-Hwang-etal_2015.pdf` — *F2FS: A New File System for Flash Storage* (Changman Lee, Dongho Sim, Joo-Young Hwang, Sangyeun Cho; USENIX FAST 2015, 2015). The canonical production flash file system: a multi-head append-only log with a Node Address Table (NAT) indirection layer to avoid the 'wandering tree' update-propagation problem in flash FS metadata indexing.
- `zns-avoiding-the-block-interface-tax-for-flash-based-ssds_Bjorling-Aghayev-Holmberg-etal_2021.pdf` — *ZNS: Avoiding the Block Interface Tax for Flash-based SSDs* (Matias Bjorling, Abutalib Aghayev, Hans Holmberg, Aravind Ramesh, Damien Le Moal, Gregory R. Ganger, George Amvrosiadis; USENIX ATC 2021, 2021). Defines zoned (sequential-write-only) storage and ports f2fs/RocksDB to it; the modern hardware contract that forces FS index/tree structures to be append-only and GC-aware, eliminating the FTL mapping table.
- `dftl-a-flash-translation-layer-employing-demand-based-selective-caching_Gupta-Kim-Urgaonkar_2009.pdf` — *DFTL: A Flash Translation Layer Employing Demand-based Selective Caching of Page-level Address Mappings* (Aayush Gupta, Youngjae Kim, Bhuvan Urgaonkar; ASPLOS 2009, 2009). Open-access canonical FTL paper: the logical-to-physical page-mapping table is itself a large on-flash index that must be demand-cached -- the same external-memory index-management problem (mapping table size vs DRAM) that FS trees face, and the layer beneath f2fs/LFS that creates write amplification.

### I-4. Journaling / write-ahead logging & crash consistency

The durability layer you bolt onto a non-CoW tree (e.g. a B-link tree) so it survives power loss: journaling, soft updates, ordering guarantees, and the write-ahead-logging recovery classic (ARIES).

- `analysis-and-evolution-of-journaling-file-systems_Prabhakaran-Arpaci-Dusseau-Arpaci-Dusseau_2005.pdf` — *Analysis and Evolution of Journaling File Systems* (Vijayan Prabhakaran, Andrea C. Arpaci-Dusseau, Remzi H. Arpaci-Dusseau; USENIX Annual Technical Conference (USENIX ATC '05), 2005). Reverse-engineers how ext3/ReiserFS/JFS/NTFS journal on-disk metadata structures (inode/bitmap blocks) to survive crashes, using semantic block-level analysis; foundational for how FS index trees stay consistent through a write-ahead log.
- `soft-updates-a-solution-to-the-metadata-update-problem-in_Ganger-McKusick-Soules-etal_2000.pdf` — *Soft Updates: A Solution to the Metadata Update Problem in File Systems* (Gregory R. Ganger, Marshall Kirk McKusick, Craig A. N. Soules, Yale N. Patt; ACM Transactions on Computer Systems (TOCS), Vol. 18, No. 2, 2000). The non-journaling alternative for crash consistency: orders dependent metadata writes (directory/inode/free-block structures) so on-disk tree state is always recoverable without a log; the canonical contrast to write-ahead logging for FS metadata.
- `optimistic-crash-consistency_Chidambaram-Pillai-Arpaci-Dusseau-etal_2013.pdf` — *Optimistic Crash Consistency* (Vijay Chidambaram, Thanumalayan Sankaranarayana Pillai, Andrea C. Arpaci-Dusseau, Remzi H. Arpaci-Dusseau; ACM Symposium on Operating Systems Principles (SOSP '13), 2013). Decouples ordering from durability in the ext4 journal (OptFS, osync/dsync), eliminating expensive flushes while keeping FS metadata trees crash-consistent; directly about how the journaling layer protecting on-disk indexes can be made cheap.
- `all-file-systems-are-not-created-equal-on-the-complexity_Pillai-Chidambaram-Alagappan-etal_2014.pdf` — *All File Systems Are Not Created Equal: On the Complexity of Crafting Crash-Consistent Applications* (Thanumalayan Sankaranarayana Pillai, Vijay Chidambaram, Ramnatthan Alagappan, Samer Al-Kiswany, Andrea C. Arpaci-Dusseau, Remzi H. Arpaci-Dusseau; USENIX Symposium on Operating Systems Design and Implementation (OSDI '14), 2014). Catalogs the per-filesystem persistence properties (atomicity/ordering of writes) that crash-consistent on-disk data structures must rely on; the Block Order Breaker (BOB) tool shows what reordering/atomicity guarantees an index-tree-on-FS can actually assume.
- `aries-a-transaction-recovery-method-supporting-fine-granularity-locking-and_Mohan-Haderle-Lindsay-etal_1992.pdf` — *ARIES: A Transaction Recovery Method Supporting Fine-Granularity Locking and Partial Rollbacks Using Write-Ahead Logging* (C. Mohan, Don Haderle, Bruce Lindsay, Hamid Pirahesh, Peter Schwarz; ACM Transactions on Database Systems (TODS), Vol. 17, No. 1, 1992). The canonical write-ahead logging / redo-undo recovery algorithm (LSNs, no-force/steal, fine-granularity page recovery) that journaling file systems and write-optimized index engines borrow to keep B-tree pages crash-consistent.

### I-5. Write-optimized key-value / LSM stores (storage context)

LSM-tree engineering for SSDs — adjacent to FS metadata write-optimization; complements the LSM original (in the sibling library) with modern designs and a survey.

- `wisckey-separating-keys-from-values-in-ssd-conscious-storage_Lu-Pillai-Gopalakrishnan-etal_2016.pdf` — *WiscKey: Separating Keys from Values in SSD-conscious Storage* (Lanyue Lu, Thanumalayan Sankaranarayana Pillai, Hariharan Gopalakrishnan, Andrea C. Arpaci-Dusseau, Remzi H. Arpaci-Dusseau; USENIX FAST 2016 (14th USENIX Conference on File and Storage Technologies), 2016). Key-value separation in an LSM tree to cut write/read amplification on SSDs; the canonical design pattern for storing large FS metadata/values out-of-line from the ordered index, directly informing how a write-optimized index inside a filesystem should lay out keys vs values.
- `pebblesdb-building-key-value-stores-using-fragmented-log-structured-merge_Raju-Kadekodi-Chidambaram-etal_2017.pdf` — *PebblesDB: Building Key-Value Stores using Fragmented Log-Structured Merge Trees* (Pandian Raju, Rohan Kadekodi, Vijay Chidambaram, Ittai Abraham; ACM SOSP 2017 (26th Symposium on Operating Systems Principles), 2017). Fragmented LSM (guard-based, skip-list-like leveling) that roughly halves write amplification while keeping ordered indexing; a concrete tree/index-layout technique for reducing compaction-driven block I/O in a write-optimized FS metadata store.
- `lsm-based-storage-techniques-a-survey_1812.07527.pdf` — *LSM-based Storage Techniques: A Survey* (Chen Luo, Michael J. Carey; The VLDB Journal 29(1):393-418, 2020 (preprint arXiv 1812.07527, 2018), 2020). The reference taxonomy of LSM-tree improvements (write amplification, compaction policies, merge scheduling, secondary indexing) spanning DB and OS/filesystem work; the map for choosing a write-optimized index backend and understanding its block-I/O trade-offs.

### Filesystem use — which to reach for

- **Persistent-memory (NVM/PMEM) filesystem** (NOVA/BPFS/DAX class): anchor on
  **FAST & FAIR** (C) for a failure-atomic, log-free, range-scannable metadata
  B+tree, with **PMwCAS** (C) as the multi-word atomic primitive and the
  **Persistent Memory I/O Primitives** (C) for the journal/propagation layer.
- **Conventional block-device filesystem**: take the **B-link tree** (B,
  Lehman-Yao) as the concurrent on-disk skeleton, then add durability via either
  **copy-on-write / shadow-paging** (I-1, the btrfs/ZFS route) or **journaling**
  (I-4). For metadata-churn-heavy workloads, a **Bε-tree filesystem** (I-2)
  trades a tunable write-amplification/query balance.
- **In-DRAM page-cache / metadata-cache index** (volatile, in front of any of the
  above): use optimistic lock coupling (B, ART-sync) + **RCU/QSBR** reclamation
  (E, Hart) — the pattern the Linux kernel already uses for dcache/inode lookups.

---

## Relevance to `portable-collections`

The workspace ships `no_std`, **`unsafe`-free**, dependency-light, generic
collections (`#![forbid(unsafe_code)]` workspace-wide). A lock-free B+tree is in
tension with that policy, so these papers split into three buckets:

- **Designs to study, but not portable verbatim.** Every lock-free B+tree here
  (A) and the non-blocking trees (D) are built on raw atomics + `unsafe` memory
  reclamation. Their *algorithms* — B-link sibling pointers (Lehman-Yao), Bw-tree
  delta chains + mapping table, BzTree's multi-word CAS, LLX/SCX cooperative
  helping — are the transferable part.
- **The blocker to solve first: reclamation (E).** In a GC-less, `unsafe`-free
  crate you cannot hand-roll hazard pointers or epochs. The realistic path is to
  lean on a vetted crate (`crossbeam-epoch` for EBR — the `practical-lock-freedom`
  / `there-has-to-be-a-better-way` line; or a hazard-pointer crate) **behind a
  feature flag**, exactly as the bimap line deferred raw-pointer ART in favour of
  a flat-`Vec` radix backend. No safe lock-free tree exists without first picking
  a reclamation story.
- **Out of current scope.** NVM persistence (C) and CXL/heterogeneous memory (F)
  — kept for the landscape, not the first target.

### Most promising path for this workspace

1. **Establish the contract on a *sequential* B+tree first.** A safe, generic,
   `no_std` B+tree (cache-conscious node layout, the `checkpoint`/`truncate`
   scoped-rollback contract the other collections share) is the baseline any
   concurrent version must match — mirroring how `BTreeBimap` / `FlatRadixBimap`
   anchored the bimap line before fancier backends.
2. **Pick the concurrency model deliberately.** The two safe-Rust-friendly routes
   are (a) **optimistic lock coupling** (B, `the-art-of-practical-synchronization`)
   — not lock-free but simple, and expressible with `std`/`parking_lot` behind a
   feature; and (b) a genuinely **lock-free** design (A/D) gated on an
   external reclamation crate. OLC is the pragmatic first step; lock-free is the
   research target.
3. **Borrow the B-link / Bw-tree split protocol**, validate against the
   verification templates (H), and benchmark with the survey methodology (G)
   against `BTreeMap` and a skip list (D) on an interner-style point-lookup +
   scoped push/pop workload.
