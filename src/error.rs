use std::time::Duration;

use crate::List;

/// Errors returned by list loading, screening, and optional integrations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The input is not well-formed XML.
    #[error("invalid XML: {0}")]
    Xml(#[from] roxmltree::Error),
    /// The XML uses an unsupported structure or invalid value.
    #[error("invalid sanctions document: {0}")]
    InvalidDocument(&'static str),
    /// A required field is missing, empty, repeated, or malformed.
    #[error("invalid or missing field: {0}")]
    InvalidField(&'static str),
    /// A configured resource limit was exceeded.
    #[error("{resource} exceeds limit {limit}")]
    LimitExceeded {
        /// The resource being bounded.
        resource: &'static str,
        /// The configured maximum.
        limit: usize,
    },
    /// The publisher's declared record count disagrees with the document.
    #[error("record count mismatch: declared {declared}, parsed {actual}")]
    RecordCount {
        /// Publisher count.
        declared: usize,
        /// Parsed count.
        actual: usize,
    },
    /// Entity IDs must be unique within one list.
    #[error("duplicate entity identifier in {0}")]
    DuplicateEntity(List),
    /// A list was supplied more than once.
    #[error("duplicate list: {0}")]
    DuplicateList(List),
    /// A required list was not supplied.
    #[error("required list is missing: {0}")]
    MissingList(List),
    /// At least one required list must be specified.
    #[error("at least one required list must be specified")]
    EmptyCoverage,
    /// The query is empty or contains no searchable information.
    #[error("query contains no searchable characters")]
    EmptyQuery,
    /// A numeric threshold or other configuration is invalid.
    #[error("invalid configuration: {0}")]
    InvalidConfiguration(&'static str),
    /// The snapshot is older than the caller's policy permits.
    #[error("{list} snapshot age {age:?} exceeds maximum {max_age:?}")]
    StaleSnapshot {
        /// Affected list.
        list: List,
        /// Time since successful acquisition or validation.
        age: Duration,
        /// Maximum permitted age.
        max_age: Duration,
    },
    /// A supplied time precedes a recorded acquisition or validation time.
    #[error("clock precedes snapshot validation time for {0}")]
    ClockSkew(List),
}

/// The library's result type.
pub type Result<T> = std::result::Result<T, Error>;
