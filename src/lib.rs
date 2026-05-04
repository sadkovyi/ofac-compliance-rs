#![doc = include_str!("../README.md")]

mod error;
mod model;
mod normalize;
mod parse;

pub use error::{Error, Result};
pub use model::{Address, Alias, Country, DateOfBirth, Entry, Identifier, List, SourceField};
pub use parse::ParseLimits;

mod snapshot;
pub use snapshot::{Freshness, FreshnessPolicy, Snapshot, SnapshotMetadata};

mod screen;
pub use screen::{Candidate, Evidence, MatchMethod, NameOptions, Report, Screener, SourceStatus};

/// Version of normalization, scoring, and ranking semantics.
pub const ALGORITHM_VERSION: &str = "ofac-compliance/1";
