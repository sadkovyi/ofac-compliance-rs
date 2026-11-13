//! Stable-Rust benchmark harness with separate parse, index, and query workloads.
use std::{
    hint::black_box,
    num::NonZeroUsize,
    time::{Duration, Instant, UNIX_EPOCH},
};

use ofac_compliance::{FreshnessPolicy, List, NameOptions, Screener, Snapshot};

fn synthetic_xml(entries: usize) -> Vec<u8> {
    let mut xml = format!(
        "<sdnList><publshInformation><Record_Count>{entries}</Record_Count></publshInformation>"
    );
    for id in 0..entries {
        xml.push_str(&format!("<sdnEntry><uid>{id}</uid><lastName>Example Trading {id:06}</lastName><sdnType>Entity</sdnType><akaList><aka><lastName>Example Holdings {id:06}</lastName></aka><aka><lastName>Example Maritime {id:06}</lastName></aka></akaList><idList><id><idType>Registration</idType><idNumber>REG-{id:06}</idNumber></id></idList></sdnEntry>"));
    }
    xml.push_str("</sdnList>");
    xml.into_bytes()
}

fn measure(name: &str, iterations: usize, mut operation: impl FnMut(usize)) {
    for i in 0..iterations.min(3) {
        operation(i);
    }
    let mut samples = Vec::with_capacity(5);
    for _ in 0..5 {
        let start = Instant::now();
        for i in 0..iterations {
            operation(i);
        }
        samples.push(start.elapsed().as_nanos() as f64 / iterations as f64);
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "{name:20} median={:12.0} ns/op  range={:.0}..{:.0}  iterations={iterations}",
        samples[2], samples[0], samples[4]
    );
}

fn main() {
    let count: usize = std::env::var("OFAC_BENCH_ENTRIES")
        .ok()
        .map(|s| s.parse().expect("entry count must be an integer"))
        .unwrap_or(10_000);
    assert!(count > 0);
    let xml = synthetic_xml(count);
    let snapshot = Snapshot::from_xml(List::Sdn, &xml, UNIX_EPOCH).unwrap();
    let make_screener = || {
        Screener::new(
            [snapshot.clone()],
            &[List::Sdn],
            FreshnessPolicy::RequireFresh(Duration::from_secs(60)),
        )
        .unwrap()
    };
    let screener = make_screener();
    let queries: Vec<_> = (0..256.min(count))
        .map(|id| format!("Example Trading {id:06}"))
        .collect();
    let ids: Vec<_> = (0..queries.len())
        .map(|id| format!("REG-{id:06}"))
        .collect();
    for (id, query) in queries.iter().enumerate() {
        let report = screener
            .screen_name_at(query, NameOptions::exact(), UNIX_EPOCH)
            .unwrap();
        assert_eq!(report.total_candidates, 1);
        assert_eq!(report.candidates[0].entry.uid, id.to_string());
    }
    let fuzzy = NameOptions::fuzzy(0.92)
        .unwrap()
        .with_limit(NonZeroUsize::new(10).unwrap());
    println!(
        "entries={count} names={} xml_bytes={} fuzzy_threshold=0.92",
        count * 3,
        xml.len()
    );
    measure("parse", 3, |_| {
        black_box(Snapshot::from_xml(List::Sdn, black_box(&xml), UNIX_EPOCH).unwrap());
    });
    measure("build indexes", 3, |_| {
        black_box(make_screener());
    });
    measure("exact name", 10_000, |i| {
        black_box(
            screener
                .screen_name_at(
                    black_box(&queries[i % queries.len()]),
                    NameOptions::exact(),
                    UNIX_EPOCH,
                )
                .unwrap(),
        );
    });
    measure("exact identifier", 10_000, |i| {
        black_box(
            screener
                .lookup_identifier_at(black_box(&ids[i % ids.len()]), None, UNIX_EPOCH)
                .unwrap(),
        );
    });
    measure("batch (256 names)", 50, |_| {
        black_box(
            screener
                .screen_batch_at(black_box(&queries), NameOptions::exact(), UNIX_EPOCH)
                .unwrap(),
        );
    });
    measure("fuzzy name", 10, |i| {
        black_box(
            screener
                .screen_name_at(black_box(&queries[i % queries.len()]), fuzzy, UNIX_EPOCH)
                .unwrap(),
        );
    });
}
