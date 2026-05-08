//! Public screening behavior and concurrency guarantees.
use std::{
    num::NonZeroUsize,
    time::{Duration, UNIX_EPOCH},
};

use ofac_compliance::{
    Error, Evidence, FreshnessPolicy, List, MatchMethod, NameOptions, Screener, Snapshot,
};

const SDN: &[u8] = include_bytes!("fixtures/sdn.xml");
const CONSOLIDATED: &[u8] = include_bytes!("fixtures/consolidated.xml");
const MAX_AGE: Duration = Duration::from_secs(60);

fn sdn() -> Snapshot {
    Snapshot::from_xml(List::Sdn, SDN, UNIX_EPOCH).unwrap()
}
fn cons() -> Snapshot {
    Snapshot::from_xml(List::Consolidated, CONSOLIDATED, UNIX_EPOCH).unwrap()
}
fn screener() -> Screener {
    Screener::new(
        [sdn(), cons()],
        &[List::Sdn, List::Consolidated],
        FreshnessPolicy::RequireFresh(MAX_AGE),
    )
    .unwrap()
}

#[test]
fn exact_search_normalizes_case_punctuation_and_compatibility_characters() {
    let screener = screener();
    for query in [
        "  EXAMPLE  TRADING, LTD. ",
        "Ｅｘａｍｐｌｅ Trading Ltd",
        "Example Trading Ltd",
    ] {
        let report = screener
            .screen_name_at(query, NameOptions::exact(), UNIX_EPOCH)
            .unwrap();
        assert_eq!(report.total_candidates, 3);
        assert_eq!(report.candidates[0].entry.uid, "200");
        assert_eq!(report.candidates[1].entry.uid, "300");
        assert_eq!(report.candidates[2].list, List::Consolidated);
        assert_eq!(report.candidates[2].entry.uid, "100");
        assert_eq!(report.sources.len(), 2);
    }
}

#[test]
fn canonical_unicode_and_non_latin_names_match_without_transliteration() {
    let screener = screener();
    for (query, uid) in [
        ("JOSE\u{301} EXAMPLE", "100"),
        ("ХОСЕ ПРИМЕР", "100"),
        ("示例贸易", "300"),
        ("أمينة مثال", "400"),
    ] {
        let report = screener
            .screen_name_at(query, NameOptions::exact(), UNIX_EPOCH)
            .unwrap();
        assert_eq!(report.candidates[0].entry.uid, uid);
    }
    assert_eq!(
        screener
            .screen_name_at("Jose Example", NameOptions::exact(), UNIX_EPOCH)
            .unwrap()
            .total_candidates,
        0
    );
}

#[test]
fn weak_aliases_are_explainable_and_can_be_excluded() {
    let screener = screener();
    let report = screener
        .screen_name_at("El Ejemplo", NameOptions::exact(), UNIX_EPOCH)
        .unwrap();
    assert!(matches!(
        report.candidates[0].evidence,
        Evidence::Name {
            alias_index: Some(1),
            method: MatchMethod::NormalizedExact,
            ..
        }
    ));
    assert_eq!(
        screener
            .screen_name_at(
                "El Ejemplo",
                NameOptions::exact().with_weak_aliases(false),
                UNIX_EPOCH
            )
            .unwrap()
            .total_candidates,
        0
    );
}

#[test]
fn fuzzy_matching_handles_typos_and_token_order_with_explicit_evidence() {
    let screener = screener();
    let fuzzy = NameOptions::fuzzy(0.9).unwrap();
    let typo = screener
        .screen_name_at("Exampel Trading Ltd", fuzzy, UNIX_EPOCH)
        .unwrap();
    assert_eq!(typo.candidates[0].entry.uid, "200");
    assert!(
        matches!(typo.candidates[0].evidence, Evidence::Name { method: MatchMethod::JaroWinkler, score, .. } if (0.9..1.0).contains(&score))
    );
    let reordered = screener
        .screen_name_at("Example José", NameOptions::fuzzy(1.0).unwrap(), UNIX_EPOCH)
        .unwrap();
    assert!(matches!(
        reordered.candidates[0].evidence,
        Evidence::Name {
            method: MatchMethod::TokenOrder,
            score: 1.0,
            ..
        }
    ));
    assert_eq!(
        screener
            .screen_name_at(
                "Example José",
                NameOptions::fuzzy(1.0).unwrap().with_token_order(false),
                UNIX_EPOCH
            )
            .unwrap()
            .total_candidates,
        0
    );
}

#[test]
fn no_match_and_deceptive_near_match_respect_the_chosen_mode() {
    let screener = screener();
    for query in ["Unrelated Oceanic Workshops", "Exampel Trading Ltd"] {
        assert!(
            screener
                .screen_name_at(query, NameOptions::exact(), UNIX_EPOCH)
                .unwrap()
                .candidates
                .is_empty()
        );
    }
    assert!(
        screener
            .screen_name_at(
                "Exampel Trading Ltd",
                NameOptions::fuzzy(1.0).unwrap(),
                UNIX_EPOCH
            )
            .unwrap()
            .candidates
            .is_empty()
    );
}

#[test]
fn rejects_invalid_thresholds_and_unusable_queries() {
    for threshold in [f64::NAN, f64::INFINITY, -1.0, 0.0, 1.01] {
        assert!(NameOptions::fuzzy(threshold).is_err());
    }
    let screener = screener();
    for query in ["", " \n\t", "---", "\u{301}"] {
        assert!(
            screener
                .screen_name_at(query, NameOptions::exact(), UNIX_EPOCH)
                .is_err()
        );
    }
    assert!(
        screener
            .screen_name_at(&"a".repeat(4097), NameOptions::exact(), UNIX_EPOCH)
            .is_err()
    );
}

#[test]
fn limits_are_applied_after_deduplication_and_total_count_is_reported() {
    let screener = screener();
    let report = screener
        .screen_name_at(
            "Example Trading Ltd",
            NameOptions::exact().with_limit(NonZeroUsize::new(1).unwrap()),
            UNIX_EPOCH,
        )
        .unwrap();
    assert_eq!(report.total_candidates, 3);
    assert_eq!(report.candidates.len(), 1);
    assert!(report.truncated());
    assert!(matches!(
        report.candidates[0].evidence,
        Evidence::Name {
            alias_index: None,
            ..
        }
    ));
}

#[test]
fn coverage_is_required_and_ids_remain_scoped_to_list() {
    assert!(matches!(
        Screener::new(
            [sdn()],
            &[List::Sdn, List::Consolidated],
            FreshnessPolicy::RequireFresh(MAX_AGE)
        ),
        Err(Error::MissingList(List::Consolidated))
    ));
    assert!(matches!(
        Screener::new(
            [sdn(), sdn()],
            &[List::Sdn],
            FreshnessPolicy::RequireFresh(MAX_AGE)
        ),
        Err(Error::DuplicateList(List::Sdn))
    ));
    assert!(Screener::new([sdn()], &[], FreshnessPolicy::RequireFresh(MAX_AGE)).is_err());
    assert!(
        Screener::new(
            [sdn()],
            &[List::Sdn, List::Sdn],
            FreshnessPolicy::RequireFresh(MAX_AGE)
        )
        .is_err()
    );
    assert_eq!(screener().len(), 5);
}

#[test]
fn freshness_is_checked_on_every_query_including_no_match() {
    let screener = screener();
    let at_limit = UNIX_EPOCH + MAX_AGE;
    assert!(
        screener
            .screen_name_at("Nobody", NameOptions::exact(), at_limit)
            .is_ok()
    );
    let stale = at_limit + Duration::from_nanos(1);
    assert!(matches!(
        screener.screen_name_at("Nobody", NameOptions::exact(), stale),
        Err(Error::StaleSnapshot { .. })
    ));
}

#[test]
fn stale_opt_in_and_clock_skew_are_explicit() {
    let screener =
        Screener::new([sdn()], &[List::Sdn], FreshnessPolicy::AllowStale(MAX_AGE)).unwrap();
    let report = screener
        .screen_name_at(
            "Nobody",
            NameOptions::exact(),
            UNIX_EPOCH + Duration::from_secs(100),
        )
        .unwrap();
    assert!(report.sources[0].freshness.stale);
    assert_eq!(report.sources[0].freshness.age, Duration::from_secs(100));
    assert!(matches!(
        screener.screen_name_at(
            "Nobody",
            NameOptions::exact(),
            UNIX_EPOCH - Duration::from_secs(1)
        ),
        Err(Error::ClockSkew(List::Sdn))
    ));
}

#[test]
fn construction_order_does_not_change_identity_or_results() {
    let a = screener();
    let b = Screener::new(
        [cons(), sdn()],
        &[List::Consolidated, List::Sdn],
        FreshnessPolicy::RequireFresh(MAX_AGE),
    )
    .unwrap();
    assert_eq!(a.snapshot_id(), b.snapshot_id());
    let ids = |s: &Screener| {
        s.screen_name_at(
            "Example Trading Ltd",
            NameOptions::fuzzy(0.8).unwrap(),
            UNIX_EPOCH,
        )
        .unwrap()
        .candidates
        .into_iter()
        .map(|c| (c.list, c.entry.uid.clone()))
        .collect::<Vec<_>>()
    };
    assert_eq!(ids(&a), ids(&b));
}
