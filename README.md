# ofac-compliance

A Rust library for screening against OFAC sanctions publications.

Requires Rust 1.89 or later. Licensed under MIT OR Apache-2.0.

## Source data

Parse standard SDN and consolidated non-SDN XML. Retain source IDs, primary names, aliases, identifying attributes, program tags, and unmodeled fields. The caller must identify the publication because both exports use the same XML structure.

Formats are described by [OFAC’s Sanctions List Service](https://ofac.treasury.gov/sanctions-list-service) and [standard XML schema](https://sanctionslistservice.ofac.treas.gov/api/PublicationPreview/exports/XML.xsd). Tests use synthetic records.

Parser limits bound bytes, nodes, depth, records, text, and names. Duplicate IDs, count mismatches, empty publications, DTDs, and malformed XML fail explicitly.

## Screening

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

Exact matching uses NFKC, Unicode lowercase, and separator collapse. Diacritics and non-Latin scripts are retained. Required publications must be present; freshness is evaluated for every query. Results include the matched name, original source entity, age, and content identity.

Weak aliases are included unless disabled. Results are deduplicated by entity, ranked deterministically, and unlimited unless `with_limit` is set; `total_candidates` and `truncated()` expose omissions. Queries are bounded to 4 KiB.

`NameOptions::fuzzy(threshold)?` evaluates Jaro-Winkler across all names with optional sorted-token comparison. Thresholds in `(0, 1]` are similarity scores, not probabilities or OFAC search scores. Calibrate them on labeled examples.

`lookup_identifier` preserves identifier case and punctuation and accepts an optional normalized type filter. Dates and addresses remain review evidence rather than automatic exclusion rules.

`screen_batch_at` uses one timestamp and fails the batch on invalid input. A screener can be shared between threads without query-side locks.

Enable `serde` to serialize records, provenance, policy, and reports. Snapshots cannot be deserialized around their validation boundary.

`Snapshot::from_reader` and its configurable-limit counterpart support bounded file and reader imports. Caller-supplied acquisition timestamps must describe the bytes, not the time an old file was opened.

## Acquisition

The optional `http` feature adds a blocking HTTPS client with request deadlines, redirect limits, and bounded bodies. No network activity starts automatically. Async applications should use a blocking pool for acquisition.

`HttpClient::refresh(list, previous)` uses matching source validators when available. A valid 304 advances validation time without replacing bytes or retrieval time. Failure leaves the old snapshot untouched.

## Development

Run `cargo fmt --all -- --check`, `cargo test --all-features`, and `cargo clippy --all-targets --all-features -- -D warnings`. Ordinary tests run offline after fetching dependencies. Screening returns candidates for review; it does not establish a legal determination.
