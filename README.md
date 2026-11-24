# ofac-compliance

Parse OFAC sanctions publications and screen names and identifiers in Rust.
Queries run against immutable, reusable snapshots, without network or disk I/O.

- Standard SDN and consolidated non-SDN XML, including aliases and identifying fields.
- Indexed exact name and identifier lookup; configurable fuzzy name matching.
- Results include source IDs, matched names, scores, snapshot digests, and freshness.
- Optional HTTPS acquisition and an atomic disk cache. No default features or async runtime.

Requires Rust 1.89 or later. The API is pre-1.0.

## Getting started

Add the library to your project:

```toml
[dependencies]
ofac-compliance = "0.1"
```

```rust
use std::time::{Duration, SystemTime};
use ofac_compliance::{FreshnessPolicy, List, NameOptions, Screener, Snapshot};

// Synthetic example. In an application, read a complete OFAC XML export.
let xml = br#"<sdnList>
  <sdnEntry>
    <uid>100</uid>
    <lastName>Example Trading Ltd</lastName>
    <sdnType>Entity</sdnType>
  </sdnEntry>
</sdnList>"#;

let acquired_at = SystemTime::now();
let snapshot = Snapshot::from_xml(List::Sdn, xml, acquired_at)?;
let screener = Screener::new(
    [snapshot],
    &[List::Sdn],
    FreshnessPolicy::RequireFresh(Duration::from_secs(24 * 60 * 60)),
)?;

let report = screener.screen_name("EXAMPLE TRADING, LTD.", NameOptions::exact())?;
assert_eq!(report.candidates[0].entry.uid, "100");
assert!(!report.truncated());
# Ok::<(), ofac_compliance::Error>(())
```

Supply both `List::Sdn` and `List::Consolidated` as requirements when both are
needed. Construction fails if a required publication is missing. Each query
checks freshness, including queries with no matches. Caller-supplied timestamps
must describe when the bytes were actually acquired; parsing an old file does
not make its contents fresh.

## Matching

`NameOptions::exact()` uses Unicode NFKC normalization and Unicode lowercase.
Runs of characters other than letters, numbers, and combining marks become one
space. Diacritics and non-Latin scripts are preserved.
It does not transliterate or perform full Unicode case folding.

`NameOptions::fuzzy(threshold)?` evaluates Jaro-Winkler similarity against every
primary name and alias. It also compares sorted tokens by default, so reordered
names can match. Thresholds are explicit, in `(0, 1]`, and are **not probabilities**
or OFAC search scores. Select a threshold using labeled examples from your use
case. There is no universal recommended threshold.

Weak aliases are included and labeled in source data. Use
`with_weak_aliases(false)` to exclude them. Each entity appears once, with its
best match. Ties prefer the primary name, then source alias order. Result ordering
is stable: descending score, list, then lexicographic entity ID.

`lookup_identifier(value, kind)` performs case-sensitive, punctuation-sensitive
equality after trimming outer whitespace. The optional type filter is normalized
like a name. This preserves distinctions in case-sensitive identifiers such as
digital currency addresses. Dates, addresses, countries, and remarks are exposed
for review; they do not silently suppress name candidates.

Reports record the algorithm and Unicode data versions. Results are unlimited
unless `with_limit(NonZeroUsize)` is set. Even with a limit,
`total_candidates` reports all qualifying entities and `truncated()` indicates
omissions. Query inputs are limited to 4 KiB. Keep the original query alongside
the report when recording an audit; the library does not log or store queries.

## Optional features

| Feature | Adds |
| --- | --- |
| `http` | Blocking HTTPS downloads, conditional refresh, timeouts, bounded retries |
| `cache` | Raw XML and metadata cache, integrity checks, file locking, atomic replacement |
| `serde` | Serialization of source records, metadata, matching policies, and reports |

`cache` enables `serde`. Features can be used independently. The cache stores no
screening results or indexes, so every load uses the current parser and every
screener builds indexes with the current matching algorithm.

With `http`, `HttpClient::refresh(list, previous)` returns a new snapshot only
after a successful download and parse, or a valid HTTP 304 response. It leaves
the previous snapshot untouched on failure. A 304 advances `validated_at` while
preserving `retrieved_at`. Both timestamps remain distinct from the publisher's
date. No polling or background refresh starts automatically.

With `cache`, `FileCache::load` distinguishes missing data, corruption, and stale
data. `AllowStale(max_age)` explicitly permits stale results and marks them in
every report. Neither cache loading nor failed refreshes reset source age.
Cache directories should be application-owned and reside on a local filesystem
with working locks and atomic rename. See [the refresh example](examples/refresh.rs)
for explicit acquisition and persistence.

## Data coverage

The parser accepts the standard `SDN.XML` and `CONSOLIDATED.XML` publications
linked from [OFAC's Sanctions List Service](https://ofac.treasury.gov/sanctions-list-service).
It supports the current standard XML namespace, its legacy namespace, and
unqualified documents. It rejects DTDs, malformed XML, empty publications,
duplicate IDs, invalid required fields, and publisher record-count mismatches.
Input bytes, node count, depth, entry count, text, and name sizes are bounded.

Advanced/enhanced XML, CSV, delta files, and customized datasets are not supported.
The caller identifies the publication for offline imports because both standard
exports use the same root schema. Consolidated entries retain program tags;
membership alone is not treated as equivalent to an SDN blocking designation.
See [data format notes](docs/data-sources.md) for source references and limits.

Screening produces candidates for review. A no-match result is scoped to the
query, loaded publications, timestamp, and matching policy. This library does
not implement ownership analysis, the 50 Percent Rule, or transaction-specific
legal determinations.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo test --no-default-features
cargo test --all-features
cargo doc --no-deps --all-features
cargo bench --bench screening
```

Tests use synthetic fixtures and do not contact OFAC. The live download test is
ignored by default. Benchmark workloads and measurement instructions are in
[docs/performance.md](docs/performance.md). Contribution guidance is in
[CONTRIBUTING.md](CONTRIBUTING.md).

Licensed under either the Apache License, Version 2.0, or the MIT license, at
your option.

