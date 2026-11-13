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

