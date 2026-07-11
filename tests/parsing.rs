//! Parser validation against synthetic standard-XML publications.
use std::time::UNIX_EPOCH;

use ofac_compliance::{Error, List, ParseLimits, Snapshot};

const SDN: &str = include_str!("fixtures/sdn.xml");
const CONSOLIDATED: &[u8] = include_bytes!("fixtures/consolidated.xml");

fn parse(xml: &str) -> ofac_compliance::Result<Snapshot> {
    Snapshot::from_xml(List::Sdn, xml.as_bytes(), UNIX_EPOCH)
}

#[test]
fn preserves_source_attributes_and_publication_metadata() {
    let snapshot = parse(SDN).unwrap();
    let entry = &snapshot.entries()[0];
    assert_eq!(entry.name, "José Example");
    assert_eq!(entry.aliases.len(), 3);
    assert!(entry.aliases[1].is_weak());
    assert_eq!(entry.identifiers[0].number, "AB-001");
    assert_eq!(
        entry.identifiers[0].issue_date.as_deref(),
        Some("01 Jan 2020")
    );
    assert_eq!(entry.addresses[0].lines, ["1 Test Street"]);
    assert_eq!(entry.dates_of_birth[0].value, "circa 1970");
    assert_eq!(entry.dates_of_birth[0].main_entry, Some(true));
    assert_eq!(entry.nationalities[0].country, "Testland");
    assert_eq!(entry.citizenships[0].main_entry, Some(false));
    assert!(entry.remarks.as_deref().unwrap().contains(" & "));
    assert_eq!(entry.additional_fields[0].name, "placeOfBirthList");
    assert_eq!(
        snapshot.entries()[2].additional_fields[0].children[0].text,
        "Preserved extension"
    );
    assert_eq!(
        snapshot.metadata().published_date.as_deref(),
        Some("01/01/2020")
    );
    assert_eq!(snapshot.metadata().declared_count, Some(4));
    assert_eq!(snapshot.source_bytes(), SDN.as_bytes());
    assert!(snapshot.metadata().source_url.is_none());
}

#[test]
fn consolidated_preserves_identity_and_programs() {
    let snapshot = Snapshot::from_xml(List::Consolidated, CONSOLIDATED, UNIX_EPOCH).unwrap();
    assert_eq!(snapshot.metadata().list, List::Consolidated);
    assert_eq!(snapshot.entries()[0].uid, "100");
    assert_eq!(snapshot.entries()[0].programs, ["TEST-NON-SDN"]);
}

#[test]
fn accepts_legacy_unqualified_and_prefixed_namespaces() {
    let ns = "https://sanctionslistservice.ofac.treas.gov/api/PublicationPreview/exports/XML";
    for xml in [
        SDN.replace(ns, "http://tempuri.org/sdnList.xsd"),
        SDN.replace(&format!(" xmlns=\"{ns}\""), ""),
    ] {
        assert_eq!(parse(&xml).unwrap().entries().len(), 4);
    }
    let xml = format!(
        "<o:sdnList xmlns:o=\"{ns}\"><o:sdnEntry><o:uid>1</o:uid><o:lastName>Example</o:lastName><o:sdnType>Entity</o:sdnType></o:sdnEntry></o:sdnList>"
    );
    assert_eq!(parse(&xml).unwrap().entries()[0].name, "Example");
}

#[test]
fn rejects_incomplete_empty_and_unsupported_documents() {
    for xml in [
        "",
        "<sdnList>",
        "<sdnList/>",
        "<Sanctions/>",
        "<html>error</html>",
        "<sdnList xmlns='https://untrusted.invalid'><sdnEntry/></sdnList>",
        "<!DOCTYPE sdnList><sdnList/>",
        "<!DOCTYPE sdnList [<!ENTITY x 'expanded'>]><sdnList>&x;</sdnList>",
    ] {
        assert!(parse(xml).is_err(), "accepted {xml}");
    }
    assert!(parse(&SDN[..SDN.len() - 20]).is_err());
    assert!(parse(&format!("{SDN}<sdnList/>")).is_err());
}

#[test]
fn rejects_duplicate_ids_and_publisher_count_mismatch() {
    assert!(matches!(
        parse(&SDN.replace("<uid>200</uid>", "<uid>100</uid>")),
        Err(Error::DuplicateEntity(List::Sdn))
    ));
    assert!(matches!(
        parse(&SDN.replace(
            "<Record_Count>4</Record_Count>",
            "<Record_Count>5</Record_Count>"
        )),
        Err(Error::RecordCount {
            declared: 5,
            actual: 4
        })
    ));
}

#[test]
fn enforces_each_resource_limit() {
    let limits = [
        ParseLimits {
            max_bytes: 10,
            ..Default::default()
        },
        ParseLimits {
            max_nodes: 10,
            ..Default::default()
        },
        ParseLimits {
            max_depth: 2,
            ..Default::default()
        },
        ParseLimits {
            max_entries: 2,
            ..Default::default()
        },
        ParseLimits {
            max_text_bytes: 2,
            ..Default::default()
        },
        ParseLimits {
            max_name_bytes: 2,
            ..Default::default()
        },
    ];
    for limit in limits {
        assert!(
            Snapshot::from_xml_with_limits(List::Sdn, SDN.as_bytes(), UNIX_EPOCH, &limit).is_err()
        );
    }
}

#[test]
fn content_identity_is_stable_and_sensitive_to_changes() {
    let a = parse(SDN).unwrap();
    let b = parse(SDN).unwrap();
    assert_eq!(a.metadata().digest, b.metadata().digest);
    assert_eq!(a.metadata().digest.len(), 64);
    assert_ne!(
        a.metadata().digest,
        parse(&SDN.replace("José", "Jose"))
            .unwrap()
            .metadata()
            .digest
    );
}

#[test]
fn reader_import_stops_at_the_limit_and_propagates_io_errors() {
    use std::io::{self, Read};
    let limits = ParseLimits {
        max_bytes: 32,
        ..Default::default()
    };
    let mut input = io::repeat(b'a');
    assert!(matches!(
        Snapshot::from_reader_with_limits(List::Sdn, &mut input, UNIX_EPOCH, &limits),
        Err(Error::LimitExceeded { .. })
    ));
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("test read failure"))
        }
    }
    assert!(matches!(
        Snapshot::from_reader(List::Sdn, Broken, UNIX_EPOCH),
        Err(Error::Io(_))
    ));
    assert_eq!(
        Snapshot::from_reader(List::Sdn, SDN.as_bytes(), UNIX_EPOCH)
            .unwrap()
            .entries()
            .len(),
        4
    );
}
