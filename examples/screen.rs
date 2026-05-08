//! Offline, reproducible screening against a synthetic fixture.
use std::time::{Duration, SystemTime};

use ofac_compliance::{Evidence, FreshnessPolicy, List, NameOptions, Screener, Snapshot};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = Snapshot::from_xml(
        List::Sdn,
        include_bytes!("../tests/fixtures/sdn.xml"),
        SystemTime::now(),
    )?;
    let screener = Screener::new(
        [snapshot],
        &[List::Sdn],
        FreshnessPolicy::RequireFresh(Duration::from_secs(3600)),
    )?;
    // The threshold is illustrative; calibrate it against your own labeled data.
    let report = screener.screen_name("Exampel Trading Ltd", NameOptions::fuzzy(0.9)?)?;
    for candidate in &report.candidates {
        if let Evidence::Name {
            matched_name,
            score,
            method,
            ..
        } = candidate.evidence
        {
            println!(
                "{}:{}  {matched_name}  score={score:.4}  {method:?}",
                candidate.list, candidate.entry.uid
            );
        }
    }
    println!(
        "snapshot={} candidates={}",
        report.snapshot_id, report.total_candidates
    );
    Ok(())
}
