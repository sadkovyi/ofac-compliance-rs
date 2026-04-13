use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use tiny_keccak::{Hasher, Sha3};

use crate::{Entry, Error, List, ParseLimits, Result};

/// Policy for accepting the age of a successfully validated source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FreshnessPolicy {
    /// Return an error when any required snapshot is older than this age.
    RequireFresh(Duration),
    /// Return results for old snapshots, explicitly marking them stale.
    AllowStale(Duration),
}

/// Age and freshness evaluated at the time of screening or cache loading.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Freshness {
    /// Elapsed time since successful retrieval or conditional validation.
    pub age: Duration,
    /// Whether the age exceeds the policy's maximum.
    pub stale: bool,
}

/// Provenance of a complete, successfully parsed source publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotMetadata {
    /// Source publication.
    pub list: List,
    /// Source URL for HTTP downloads; absent for caller-supplied bytes.
    pub source_url: Option<String>,
    /// Time the bytes were acquired. Supplied by the caller for offline imports.
    pub retrieved_at: SystemTime,
    /// Last successful retrieval or HTTP 304 validation; never a failed attempt.
    pub validated_at: SystemTime,
    /// Publisher date text, when present; distinct from local acquisition time.
    pub published_date: Option<String>,
    /// Lowercase SHA3-256 digest of the original bytes.
    pub digest: String,
    /// Publisher-declared record count, when present.
    pub declared_count: Option<usize>,
}

/// An immutable, validated list. Clones share the entries and original bytes.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub(crate) metadata: SnapshotMetadata,
    pub(crate) entries: Arc<[Entry]>,
    pub(crate) raw: Arc<[u8]>,
}

impl Snapshot {
    /// Parse standard OFAC XML with default resource limits.
    ///
    /// The list identity is supplied by the caller because both exports use the
    /// same XML structure. Only pass a timestamp that describes these bytes.
    pub fn from_xml(list: List, bytes: &[u8], retrieved_at: SystemTime) -> Result<Self> {
        Self::from_xml_with_limits(list, bytes, retrieved_at, &ParseLimits::default())
    }

    /// Parse a complete XML document with explicit resource limits.
    pub fn from_xml_with_limits(
        list: List,
        bytes: &[u8],
        retrieved_at: SystemTime,
        limits: &ParseLimits,
    ) -> Result<Self> {
        let parsed = crate::parse::parse(list, bytes, limits)?;
        Ok(Self {
            metadata: SnapshotMetadata {
                list,
                source_url: None,
                retrieved_at,
                validated_at: retrieved_at,
                published_date: parsed.published_date,
                digest: digest(bytes),
                declared_count: parsed.declared_count,
            },
            entries: parsed.entries.into(),
            raw: bytes.into(),
        })
    }

    /// Publication and integrity metadata.
    pub fn metadata(&self) -> &SnapshotMetadata {
        &self.metadata
    }

    /// Entries in source order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The original document, including fields not modeled by this crate.
    pub fn source_bytes(&self) -> &[u8] {
        &self.raw
    }

    /// Evaluate age without changing the snapshot or its timestamps.
    pub fn freshness(&self, policy: FreshnessPolicy, now: SystemTime) -> Result<Freshness> {
        let age = now
            .duration_since(self.metadata.validated_at)
            .map_err(|_| Error::ClockSkew(self.metadata.list))?;
        let (max_age, allow_stale) = match policy {
            FreshnessPolicy::RequireFresh(age) => (age, false),
            FreshnessPolicy::AllowStale(age) => (age, true),
        };
        let stale = age > max_age;
        if stale && !allow_stale {
            return Err(Error::StaleSnapshot {
                list: self.metadata.list,
                age,
                max_age,
            });
        }
        Ok(Freshness { age, stale })
    }
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    let mut hash = Sha3::v256();
    hash.update(bytes);
    let mut output = [0; 32];
    hash.finalize(&mut output);
    let mut hex = String::with_capacity(64);
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    for byte in output {
        hex.push(DIGITS[usize::from(byte >> 4)] as char);
        hex.push(DIGITS[usize::from(byte & 15)] as char);
    }
    hex
}
