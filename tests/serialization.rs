//! Reports serialize their evidence and policy without serializing index state.
#![cfg(feature = "serde")]

use ofac_compliance::{FreshnessPolicy, List, NameOptions, Screener, Snapshot};
use std::time::{Duration, UNIX_EPOCH};

#[test]
fn serialized_reports_retain_evidence_coverage_and_query_policy() {
    let snapshot =
        Snapshot::from_xml(List::Sdn, include_bytes!("fixtures/sdn.xml"), UNIX_EPOCH).unwrap();
    let screener = Screener::new(
        [snapshot],
        &[List::Sdn],
        FreshnessPolicy::RequireFresh(Duration::from_secs(60)),
    )
    .unwrap();
    let report = screener
        .screen_name_at("El Ejemplo", NameOptions::exact(), UNIX_EPOCH)
        .unwrap();
    let value = serde_json::to_value(&report).unwrap();
    assert_eq!(value["total_candidates"], 1);
    assert_eq!(value["candidates"][0]["entry"]["uid"], "100");
    assert_eq!(value["candidates"][0]["evidence"]["Name"]["alias_index"], 1);
    assert_eq!(
        value["sources"][0]["metadata"]["digest"],
        screener.snapshots()[0].metadata().digest
    );
    assert_eq!(value["sources"][0]["freshness"]["stale"], false);
    assert!(value["name_options"]["threshold"].is_null());
    let lookup = screener
        .lookup_identifier_at("AB-001", Some("Passport"), UNIX_EPOCH)
        .unwrap();
    let value = serde_json::to_value(&lookup).unwrap();
    assert_eq!(value["identifier_kind"], "passport");
    assert!(value["name_options"].is_null());
}
