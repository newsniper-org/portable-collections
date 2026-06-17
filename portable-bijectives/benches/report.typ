#set document(
  title: "Bimap backend benchmark: BTreeBimap vs FlatRadixBimap",
  author: "윤병익",
)
#set page(paper: "a4", margin: 2.2cm, numbering: "1")
#set par(justify: true, leading: 0.62em)
#set text(size: 10pt)
#set heading(numbering: "1.")
#show heading.where(level: 1): set text(13pt)

#align(center)[
  #text(18pt, weight: "bold")[Bimap backend benchmark]
  #v(0.3em)
  #text(11pt)[#raw("BTreeBimap") (BTreeMap-backed) vs #raw("FlatRadixBimap") (dense #raw("Vec") radix)]
  #v(0.2em)
  #text(9.5pt, fill: luma(110))[on the SMT-solver term↔var interner workload — dense #raw("u32") ids, #raw("N = 10000")]
]

#v(0.8em)

// Criterion's own point estimates (all times in nanoseconds), loaded verbatim
// from the generated estimates.json — no hand-entered numbers.
#let est(group, bench) = json("criterion-data/" + group + "-" + bench + ".json")

#let fmt(ns) = {
  if ns >= 1e6 { [#calc.round(ns / 1e6, digits: 3)~ms] }
  else if ns >= 1e3 { [#calc.round(ns / 1e3, digits: 3)~µs] }
  else { [#calc.round(ns, digits: 2)~ns] }
}

#let mean-of(g, b) = est(g, b).mean.point_estimate
#let speedup(g) = calc.round(mean-of(g, "BTreeBimap") / mean-of(g, "FlatRadixBimap"), digits: 1)

= Summary

Across all three hot interner operations the dense-`Vec` radix backend beats the
tree backend by an order of magnitude — confirming the research survey's
prediction that for dense, monotonically-minted ids the id-as-index layout wins
decisively over an (already cache-conscious) `BTreeMap`.

#figure(
  table(
    columns: (auto, auto, auto, auto),
    align: (left, right, right, right),
    inset: 7pt,
    stroke: 0.5pt + luma(180),
    table.header([*Operation*], [*`BTreeBimap`*], [*`FlatRadixBimap`*], [*Speed-up*]),
    [Insert (10000 pairs)], fmt(mean-of("insert_dense", "BTreeBimap")), fmt(mean-of("insert_dense", "FlatRadixBimap")), [#speedup("insert_dense")×],
    [Lookup (10000 hits)], fmt(mean-of("lookup_dense", "BTreeBimap")), fmt(mean-of("lookup_dense", "FlatRadixBimap")), [#speedup("lookup_dense")×],
    [Scoped push/pop], fmt(mean-of("scoped_rollback", "BTreeBimap")), fmt(mean-of("scoped_rollback", "FlatRadixBimap")), [#speedup("scoped_rollback")×],
  ),
  caption: [Mean wall-clock time per batch (Criterion point estimate; lower is better). Speed-up is #raw("BTreeBimap") mean ÷ #raw("FlatRadixBimap") mean.],
)

= Detailed estimates

Every figure below is Criterion's own statistic, read verbatim from the
generated `estimates.json` via Typst's `json()`.

#let detail-rows(g) = {
  let mk(b) = {
    let e = est(g, b)
    (
      raw(b),
      fmt(e.mean.point_estimate),
      [#fmt(e.mean.confidence_interval.lower_bound) – #fmt(e.mean.confidence_interval.upper_bound)],
      fmt(e.median.point_estimate),
      fmt(e.std_dev.point_estimate),
    )
  }
  (mk("BTreeBimap"), mk("FlatRadixBimap"))
}

#figure(
  table(
    columns: 5,
    align: (left, right, center, right, right),
    inset: 6pt,
    stroke: 0.5pt + luma(180),
    table.header([*Backend*], [*Mean*], [*95% CI of mean*], [*Median*], [*Std dev*]),
    table.cell(colspan: 5, fill: luma(238))[*Insert — 10000 pairs*],
    ..detail-rows("insert_dense").flatten(),
    table.cell(colspan: 5, fill: luma(238))[*Lookup — 10000 hits*],
    ..detail-rows("lookup_dense").flatten(),
    table.cell(colspan: 5, fill: luma(238))[*Scoped push/pop*],
    ..detail-rows("scoped_rollback").flatten(),
  ),
  caption: [Per-backend Criterion estimates.],
)

= Reading the numbers

- *Insert* and *lookup* are the cleanest wins: #raw("FlatRadixBimap") indexes by
  the id itself (`fwd[k]`, `rev[v]`), so a lookup is one array access with zero
  comparisons and a single cache miss, versus a `B = 6` tree descent with
  in-node scans.
- *Scoped push/pop* (`checkpoint` then `truncate`) is the noisiest case for
  #raw("FlatRadixBimap") — its median exceeds its mean and the standard
  deviation is large — because the batched setup dominates a sub-100 µs
  measurement. Even so it stays an order of magnitude under #raw("BTreeBimap").

= Method

Benchmarks: `portable-bijectives/benches/bimap_bench.rs` (Criterion,
`harness = false`). Each group fills or queries #raw("N = 10000") dense
#raw("u32") pairs; `scoped_rollback` fills half, checkpoints, interns the second
half inside the scope, then truncates back to the checkpoint. Raw Criterion
output lives under `target/criterion/`; the point estimates used here are copied
verbatim into `benches/criterion-data/` and read by this document.

= Conclusion

#raw("FlatRadixBimap") clears the survey's go/no-go gate decisively. Because
every heavier radix proposal (a path-copying ART, COLA, a CSB#super[+] map) must
first beat this flat baseline on the dense workload — and the margin here is
#speedup("lookup_dense")× on lookups alone — those structures are not worth
building for dense ids. They re-enter scope only if sparse, non-monotonic, or
string keys become a real requirement.
