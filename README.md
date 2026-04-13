# ofac-compliance

A Rust library for screening against OFAC sanctions publications.

Requires Rust 1.89 or later. Licensed under MIT OR Apache-2.0.

## Source data

Parse standard SDN and consolidated non-SDN XML. Retain source IDs, primary names, aliases, identifying attributes, program tags, and unmodeled fields. The caller must identify the publication because both exports use the same XML structure.

Formats are described by [OFAC’s Sanctions List Service](https://ofac.treasury.gov/sanctions-list-service) and [standard XML schema](https://sanctionslistservice.ofac.treas.gov/api/PublicationPreview/exports/XML.xsd). Tests use synthetic records.

Parser limits bound bytes, nodes, depth, records, text, and names. Duplicate IDs, count mismatches, empty publications, DTDs, and malformed XML fail explicitly.

## Snapshots

`Snapshot::from_xml` preserves the original bytes and publisher metadata with a
content digest. Clones share immutable records. Acquisition and validation time
are independent of the publisher date. `RequireFresh` rejects stale data;
`AllowStale` marks it explicitly. Future timestamps fail with `ClockSkew`.

## Development

Run `cargo fmt --all -- --check`, `cargo test --all-features`, and `cargo clippy --all-targets --all-features -- -D warnings`. Ordinary tests run offline after fetching dependencies. Screening returns candidates for review; it does not establish a legal determination.
