//! Disk-cache integrity, freshness, and concurrent replacement.
#![cfg(feature = "cache")]

use std::{
    fs,
    time::{Duration, UNIX_EPOCH},
};

use ofac_compliance::{Error, FreshnessPolicy, List, ParseLimits, Snapshot, cache::FileCache};

const XML: &[u8] = include_bytes!("fixtures/sdn.xml");
const AGE: Duration = Duration::from_secs(60);

fn snapshot(seconds: u64) -> Snapshot {
    Snapshot::from_xml(List::Sdn, XML, UNIX_EPOCH + Duration::from_secs(seconds)).unwrap()
}

#[test]
fn cache_is_optional_and_round_trip_preserves_provenance() {
    let directory = tempfile::tempdir().unwrap();
    let cache = FileCache::new(directory.path());
    assert!(
        cache
            .load(List::Sdn, FreshnessPolicy::RequireFresh(AGE), UNIX_EPOCH)
            .unwrap()
            .is_none()
    );
    let original = snapshot(0);
    cache.store(&original).unwrap();
    let hit = cache
        .load(
            List::Sdn,
            FreshnessPolicy::RequireFresh(AGE),
            UNIX_EPOCH + AGE,
        )
        .unwrap()
        .unwrap();
    assert_eq!(original.metadata(), hit.snapshot.metadata());
    assert_eq!(original.entries(), hit.snapshot.entries());
    assert_eq!(hit.snapshot.source_bytes(), XML);
    assert!(!hit.freshness.stale);
}

#[test]
fn cache_load_does_not_reset_age_and_stale_use_requires_opt_in() {
    let directory = tempfile::tempdir().unwrap();
    let cache = FileCache::new(directory.path());
    cache.store(&snapshot(0)).unwrap();
    let now = UNIX_EPOCH + Duration::from_secs(61);
    assert!(matches!(
        cache.load(List::Sdn, FreshnessPolicy::RequireFresh(AGE), now),
        Err(Error::StaleSnapshot { .. })
    ));
    let hit = cache
        .load(List::Sdn, FreshnessPolicy::AllowStale(AGE), now)
        .unwrap()
        .unwrap();
    assert!(hit.freshness.stale);
    assert_eq!(hit.snapshot.metadata().validated_at, UNIX_EPOCH);
}

#[test]
fn corrupt_payload_fails_then_can_be_replaced() {
    let directory = tempfile::tempdir().unwrap();
    let cache = FileCache::new(directory.path());
    cache.store(&snapshot(0)).unwrap();
    let path = directory.path().join("sdn.ofac-cache");
    let content = fs::read(&path).unwrap();
    let altered = String::from_utf8_lossy(&content)
        .replace("Testland", "Westland")
        .into_bytes();
    fs::write(&path, altered).unwrap();
    assert!(
        cache
            .load(List::Sdn, FreshnessPolicy::RequireFresh(AGE), UNIX_EPOCH)
            .is_err()
    );
    cache.store(&snapshot(1)).unwrap();
    assert_eq!(
        cache
            .load(
                List::Sdn,
                FreshnessPolicy::RequireFresh(AGE),
                UNIX_EPOCH + AGE
            )
            .unwrap()
            .unwrap()
            .snapshot
            .source_bytes(),
        XML
    );
}

#[test]
fn rejects_truncation_wrong_list_unknown_version_and_oversized_headers() {
    let directory = tempfile::tempdir().unwrap();
    let cache = FileCache::new(directory.path());
    cache.store(&snapshot(0)).unwrap();
    let path = directory.path().join("sdn.ofac-cache");
    let original = fs::read(&path).unwrap();
    for len in [0, 8, 12, original.len() - 10] {
        fs::write(&path, &original[..len]).unwrap();
        assert!(
            cache
                .load(List::Sdn, FreshnessPolicy::RequireFresh(AGE), UNIX_EPOCH)
                .is_err()
        );
    }
    let mut corrupted = original.clone();
    corrupted[5] = 255;
    fs::write(&path, &corrupted).unwrap();
    assert!(
        cache
            .load(List::Sdn, FreshnessPolicy::RequireFresh(AGE), UNIX_EPOCH)
            .is_err()
    );
    corrupted = original.clone();
    corrupted[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    fs::write(&path, &corrupted).unwrap();
    assert!(
        cache
            .load(List::Sdn, FreshnessPolicy::RequireFresh(AGE), UNIX_EPOCH)
            .is_err()
    );
    fs::write(directory.path().join("consolidated.ofac-cache"), &original).unwrap();
    assert!(
        cache
            .load(
                List::Consolidated,
                FreshnessPolicy::RequireFresh(AGE),
                UNIX_EPOCH
            )
            .is_err()
    );
}

#[test]
fn failed_store_retains_previous_entry() {
    let directory = tempfile::tempdir().unwrap();
    let cache = FileCache::new(directory.path());
    cache.store(&snapshot(0)).unwrap();
    let limited = FileCache::new(directory.path()).with_limits(ParseLimits {
        max_bytes: 10,
        ..Default::default()
    });
    assert!(limited.store(&snapshot(1)).is_err());
    let hit = cache
        .load(List::Sdn, FreshnessPolicy::RequireFresh(AGE), UNIX_EPOCH)
        .unwrap()
        .unwrap();
    assert_eq!(hit.snapshot.metadata().retrieved_at, UNIX_EPOCH);
}

#[test]
fn older_writer_cannot_overwrite_newer_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let cache = FileCache::new(directory.path());
    cache.store(&snapshot(20)).unwrap();
    assert!(cache.store(&snapshot(10)).is_err());
    assert_eq!(
        cache
            .load(
                List::Sdn,
                FreshnessPolicy::RequireFresh(AGE),
                UNIX_EPOCH + AGE
            )
            .unwrap()
            .unwrap()
            .snapshot
            .metadata()
            .retrieved_at,
        UNIX_EPOCH + Duration::from_secs(20)
    );
}

#[test]
fn future_timestamps_do_not_look_fresh() {
    let directory = tempfile::tempdir().unwrap();
    let cache = FileCache::new(directory.path());
    cache.store(&snapshot(20)).unwrap();
    assert!(matches!(
        cache.load(List::Sdn, FreshnessPolicy::AllowStale(AGE), UNIX_EPOCH),
        Err(Error::ClockSkew(List::Sdn))
    ));
}

#[test]
fn concurrent_writers_publish_only_complete_entries() {
    let directory = tempfile::tempdir().unwrap();
    let cache = FileCache::new(directory.path());
    cache.store(&snapshot(0)).unwrap();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let cache = &cache;
            scope.spawn(move || {
                for _ in 0..10 {
                    cache.store(&snapshot(0)).unwrap();
                    let hit = cache
                        .load(List::Sdn, FreshnessPolicy::RequireFresh(AGE), UNIX_EPOCH)
                        .unwrap()
                        .unwrap();
                    assert_eq!(hit.snapshot.source_bytes(), XML);
                }
            });
        }
    });
    let files: Vec<_> = fs::read_dir(directory.path()).unwrap().collect();
    assert_eq!(
        files.len(),
        2,
        "one cache entry and one lock; no temporary files remain"
    );
}

#[test]
fn corrupted_digest_is_detected_even_when_xml_still_parses() {
    let directory = tempfile::tempdir().unwrap();
    let cache = FileCache::new(directory.path());
    cache.store(&snapshot(0)).unwrap();
    let path = directory.path().join("sdn.ofac-cache");
    let mut bytes = fs::read(&path).unwrap();
    let needle = b"Testland";
    let position = bytes
        .windows(needle.len())
        .position(|window| window == needle)
        .unwrap();
    bytes[position] = b'W';
    fs::write(&path, bytes).unwrap();
    assert!(matches!(
        cache.load(List::Sdn, FreshnessPolicy::RequireFresh(AGE), UNIX_EPOCH),
        Err(Error::InvalidCache("content digest mismatch"))
    ));
}
