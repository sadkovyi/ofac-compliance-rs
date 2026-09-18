//! Explicit network acquisition with cache fallback controlled by the caller.
use std::time::{Duration, SystemTime};

use ofac_compliance::{
    FreshnessPolicy, List, Screener,
    cache::FileCache,
    http::{HttpClient, HttpOptions},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args_os()
        .nth(1)
        .ok_or("usage: cargo run --example refresh --features http,cache -- <cache-directory>")?;
    let cache = FileCache::new(directory);
    let client = HttpClient::new(HttpOptions::default())?;
    let max_age = Duration::from_secs(24 * 60 * 60);
    let required = [List::Sdn, List::Consolidated];
    let mut snapshots = Vec::new();
    for list in required {
        // An old cache entry may provide validators. Failure to refresh still
        // aborts this example; allowing old validators is not stale screening.
        let previous = match cache.load(
            list,
            FreshnessPolicy::AllowStale(max_age),
            SystemTime::now(),
        ) {
            Ok(hit) => hit.map(|hit| hit.snapshot),
            Err(error) => {
                eprintln!(
                    "{list} cache could not be loaded: {error}; fetching a complete publication"
                );
                None
            }
        };
        snapshots.push(client.refresh(list, previous.as_ref())?);
    }
    // Build the complete replacement before making it available to readers.
    let replacement = Screener::new(snapshots, &required, FreshnessPolicy::RequireFresh(max_age))?;
    for snapshot in replacement.snapshots() {
        cache.store(snapshot)?;
    }
    println!(
        "loaded {} source entries; snapshot {}",
        replacement.len(),
        replacement.snapshot_id()
    );
    // In a server, swap an Arc<Screener> in application-owned state here.
    Ok(())
}
