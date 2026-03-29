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

#[test]
fn rejects_duplicate_ids_count_mismatches_and_resource_excesses() {
    let xml = include_str!("fixtures/sdn.xml");
    for invalid in [
        xml.replace("<uid>200</uid>", "<uid>100</uid>"),
        xml.replace(
            "<Record_Count>4</Record_Count>",
            "<Record_Count>5</Record_Count>",
        ),
    ] {
        assert!(parse_xml(List::Sdn, invalid.as_bytes(), &ParseLimits::default()).is_err());
    }
    for limits in [
        ParseLimits {
            max_bytes: 10,
            ..Default::default()
        },
        ParseLimits {
            max_nodes: 5,
            ..Default::default()
        },
        ParseLimits {
            max_depth: 2,
            ..Default::default()
        },
        ParseLimits {
            max_entries: 1,
            ..Default::default()
        },
        ParseLimits {
            max_text_bytes: 2,
            ..Default::default()
        },
    ] {
        assert!(parse_xml(List::Sdn, xml.as_bytes(), &limits).is_err());
    }
}
