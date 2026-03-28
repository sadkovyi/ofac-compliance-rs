//! Synthetic publication parsing regressions.
use ofac_compliance::{List, ParseLimits, parse_xml};

#[test]
fn preserves_both_publications_and_identifying_fields() {
    let sdn = parse_xml(
        List::Sdn,
        include_bytes!("fixtures/sdn.xml"),
        &ParseLimits::default(),
    )
    .unwrap();
    let consolidated = parse_xml(
        List::Consolidated,
        include_bytes!("fixtures/consolidated.xml"),
        &ParseLimits::default(),
    )
    .unwrap();
    assert_eq!(sdn.entries.len(), 4);
    assert_eq!(consolidated.entries.len(), 1);
    assert_eq!(sdn.entries[0].name, "José Example");
    assert_eq!(sdn.entries[0].identifiers[0].number, "AB-001");
    assert!(sdn.entries[0].aliases[1].is_weak());
    assert_eq!(
        sdn.entries[0].remarks.as_deref(),
        Some("Synthetic & deliberately incomplete identifying data.")
    );
    assert_eq!(sdn.published_date.as_deref(), Some("01/01/2020"));
    assert_eq!(sdn.declared_count, Some(4));
}

#[test]
fn rejects_malformed_and_empty_publications() {
    for input in ["", "<sdnList>", "<sdnList/>", "<html>error</html>"] {
        assert!(parse_xml(List::Sdn, input.as_bytes(), &ParseLimits::default()).is_err());
    }
}
