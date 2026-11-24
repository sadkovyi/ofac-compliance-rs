#![doc = include_str!("../README.md")]

mod error;
mod model;
mod normalize;
mod parse;
mod screen;
mod snapshot;

#[cfg(feature = "cache")]
pub mod cache;
#[cfg(feature = "http")]
pub mod http;

pub use error::{Error, Result};
pub use model::{Address, Alias, Country, DateOfBirth, Entry, Identifier, List, SourceField};
pub use parse::ParseLimits;
pub use screen::{Candidate, Evidence, MatchMethod, NameOptions, Report, Screener, SourceStatus};
pub use snapshot::{Freshness, FreshnessPolicy, Snapshot, SnapshotMetadata};

/// Version of normalization, scoring, and ranking semantics.
pub const ALGORITHM_VERSION: &str = "ofac-compliance/1";

/// Unicode data version used by name normalization. Included in dataset identity
/// so a dependency update cannot silently reuse a prior normalization identity.
pub const UNICODE_VERSION: (u8, u8, u8) = unicode_normalization::UNICODE_VERSION;
