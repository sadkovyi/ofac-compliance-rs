//! Explicitly opted-in network smoke test; excluded from ordinary test runs.
#![cfg(feature = "http")]

use ofac_compliance::{
    List,
    http::{HttpClient, HttpOptions},
};

#[test]
#[ignore = "downloads live OFAC publications; run explicitly with network access"]
fn downloads_both_standard_publications() {
    let client = HttpClient::new(HttpOptions::default()).unwrap();
    for list in [List::Sdn, List::Consolidated] {
        let snapshot = client.refresh(list, None).unwrap();
        assert!(!snapshot.entries().is_empty());
        assert_eq!(snapshot.metadata().source_url.as_deref(), Some(list.url()));
        let refreshed = client.refresh(list, Some(&snapshot)).unwrap();
        assert!(!refreshed.entries().is_empty());
    }
}
