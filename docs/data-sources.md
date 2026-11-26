# Data sources and interpretation

The implementation targets OFAC's standard XML publications. These references
describe the source formats and their relationship to other OFAC products:

- [Sanctions List Service](https://ofac.treasury.gov/sanctions-list-service)
- [SDN publication page](https://sanctionslist.ofac.treas.gov/Home/SdnList)
- [Consolidated publication page](https://sanctionslist.ofac.treas.gov/Home/ConsolidatedList)
- [Standard XML schema](https://sanctionslistservice.ofac.treas.gov/api/PublicationPreview/exports/XML.xsd)
- [OFAC's May 2024 namespace change](https://ofac.treasury.gov/recent-actions/20240507_44)
- [Standard and advanced format comparison](https://ofac.treasury.gov/sdn-list-data-formats-data-schemas/frequently-asked-questions-on-advanced-sanctions-list-standard)

`List::url()` contains the standard XML export URLs under
`https://sanctionslistservice.ofac.treas.gov/api/PublicationPreview/exports/`:
`SDN.XML` and `CONSOLIDATED.XML`. Acquisition follows up to three HTTPS redirects.
The live integration test checks both endpoints when explicitly enabled; unit
tests do not establish current endpoint availability.

## XML contract

Both publications use `sdnList` and `sdnEntry`; the document does not uniquely
identify which export was supplied. Offline callers must supply the correct
`List`. Only current standard, legacy standard, and unqualified namespaces are
accepted. Foreign-namespace fields cannot substitute for required fields.

Each entry needs one nonempty `uid`, `sdnType`, and `lastName`. Names are assembled
from `firstName` and `lastName`, preserving the component fields. Primary names
and aliases must contain searchable characters. Repeated scalar fields, repeated
modeled containers, and duplicate entity IDs within one publication fail parsing.
The same entity ID can occur in two publications and remains two source records.

The publisher's `publshInformation/Publish_Date` is retained as text. When
`Record_Count` is present, it must equal the parsed entry count. The parser cannot
detect a whole missing entry if the producer also omits or changes that count.
It checks the complete XML structure, but does not perform full XSD validation.

Passports, national IDs, digital currency addresses, addresses, birth dates,
nationalities, and citizenships have typed containers. Dates are not coerced into
precise timestamps. Other top-level entry fields, including vessel information
and places of birth, are retained as `SourceField` trees. Complete original bytes
are available through `Snapshot::source_bytes()`, including XML attributes and
fields not represented in the typed model.

Advanced/enhanced XML, CSV, delta files, and custom subsets are outside the
current parser contract. Non-SDN program tags are preserved as source strings;
this crate does not infer the restrictions associated with a program.

## Freshness and replacement

`retrieved_at` records acquisition of the bytes. `validated_at` records the most
recent successful acquisition or conditional HTTP 304 check. The publisher's
date is independent. Freshness measures elapsed time since `validated_at`; it
does not assert that the publisher changed the list recently. Future timestamps
return `ClockSkew`, and age equal to the configured maximum is accepted.

`RequireFresh` rejects stale data. `AllowStale` returns candidates with the stale
flag and age attached to every affected source. Download failure does not reset
either timestamp. A refresh returns a new snapshot; the caller decides when to
publish a complete replacement `Screener`, for example through application-owned
`Arc` state.

Retries apply to 408, 429, 500, 502, 503, and 504 responses only. Numeric
`Retry-After` values are honored within the configured delay bound. HTTP-date
values and longer delays are returned as errors for the caller to schedule.
Transport errors, body read errors, and invalid publications are not retried.
Responses other than 200 or a valid conditional 304 are rejected. Requests use
identity encoding, and this crate enables only ureq's Rustls feature. Unsupported
content encodings exposed by ureq are rejected before reading the body. If another
dependency enables ureq's decoding features, ureq may decode the response and
remove its encoding headers. The download limit still applies to the bytes passed
to the XML parser; snapshots and digests describe those bytes.

## Cache format

The optional cache stores one file per list: a versioned header followed by raw
XML. Metadata includes list identity, source URL when known, timestamps, HTTP
validators, and the SHA3-256 digest of the exact XML bytes. This digest is distinct
from the SHA-256/SHA-384/SHA-512 signatures published on OFAC's download pages.
It identifies contents and detects corruption; it does not authenticate origin.

Cache loads check the header, source identity, timestamps, size, digest, and XML.
They reconstruct entries with the current parser. Indexes and query results are
never persisted. Format changes require a cache version change; matching changes
require an algorithm version change, not a serialized-index migration.

Writers serialize through an advisory file lock, sync a temporary file, then
atomically replace the destination. Linux/Unix writes also sync the parent
directory. A failed write before replacement retains the prior file. Older
validation timestamps cannot overwrite newer ones. The cache provides atomicity
per publication, not a transaction spanning both lists. Readers see either the
old complete entry or the new complete entry.

Each active file is bounded by `ParseLimits::max_bytes` plus a 16 KiB metadata
allowance. There are at most two active entries and one active writer temporary
file per directory. Abrupt process termination may leave orphan temporary files;
applications can clean them during maintenance when no cache operations run.
