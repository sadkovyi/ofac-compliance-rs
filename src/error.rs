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
}

/// The library's result type.
pub type Result<T> = std::result::Result<T, Error>;
