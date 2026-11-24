# Measuring performance

Run the stable-Rust benchmark harness in release mode:

```sh
cargo bench --bench screening
OFAC_BENCH_ENTRIES=50000 cargo bench --bench screening
```

The default dataset contains 10,000 synthetic entities, each with a primary name,
two aliases, and one identifier. The harness reports input bytes and median,
minimum, and maximum time per operation across five samples. It separates XML
parsing, index construction, exact-name lookup, identifier lookup, a batch of up
to 256 exact queries, and fuzzy-name lookup. Query strings are prepared before
timing. Fuzzy matching uses threshold 0.92, token-order comparison, and a limit of
ten returned entities; it still evaluates every name and reports the full count.

The synthetic dataset is deliberately reproducible. It does not approximate the
distribution of real names or establish real-world recall, precision, or latency.
Use representative permitted data and a labeled query set before choosing a
threshold or introducing a candidate-selection optimization.

Exact lookups use hash indexes over normalized names or original identifiers.
Their work scales with query length and the number of matching records. Fuzzy
search scans every primary name and included alias; no heuristic candidate
filter can silently remove a qualifying match. Its cost scales with the number
and length of names. Snapshot parsing and index construction happen outside the
screening path. A `Screener` can be shared between threads without locks.

For a benchmark report, record compiler version, target, CPU, enabled features,
entity/name counts, input bytes, and query policy. Measure resident memory
separately with an OS profiler; retained XML, parsed records, and indexes all
contribute. On Linux, an installed GNU `time` can report peak RSS for the built
benchmark executable. Do not include Cargo's compilation memory in that number.

## Initial measurement

Measured on Linux x86-64, AMD Ryzen 7 9800X3D, Rust 1.89.0, release profile,
with no optional features. Dataset: 10,000 entries, 30,000 names, 3,188,982 XML
bytes. Medians of five samples on 2026-11-23:

| Operation | Median |
| --- | ---: |
| Parse XML and compute digest | 44.29 ms |
| Build indexes from a parsed snapshot | 23.48 ms |
| Exact name lookup | 625 ns |
| Exact identifier lookup | 81 ns |
| Batch of 256 exact names | 169.00 µs |
| Fuzzy name lookup | 10.31 ms |

Whole-process peak RSS was 78,700 KiB (76.9 MiB), measured by launching the built
benchmark directly and reading `getrusage(RUSAGE_CHILDREN)`. This includes the
retained dataset, an existing screener, and temporary parse/index allocations;
it is not a steady-state per-screener memory estimate.

The measurement used local source overrides for roxmltree 0.20.0,
unicode-normalization 0.1.26, and tinyvec 1.13.3 because registry access was
unavailable. Matching used strsim 0.11.1. No HTTP or TLS code participates in
these workloads. Reproduce the measurements with the resolved dependency graph
before using them for capacity planning. Record before/after results with a
change when performance is its motivation.
