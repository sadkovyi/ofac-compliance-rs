//! Optional disk storage for original XML and acquisition metadata.
//!
//! Each directory holds at most one entry per supported list. Reads re-parse the
//! XML and verify its digest; no serialized indexes or screening queries are
//! stored. Writers use an advisory lock and atomically replace a complete file.
//! Use a local filesystem with reliable file-lock and atomic-rename semantics.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde::{Deserialize, Serialize};

use crate::{Error, Freshness, FreshnessPolicy, List, ParseLimits, Result, Snapshot};

const MAGIC: &[u8; 8] = b"OFAC\0\x01\r\n";
const MAX_HEADER_BYTES: usize = 16 * 1024;

/// A validated cache hit, with freshness evaluated at load time.
#[derive(Debug)]
pub struct CacheHit {
    /// Fully re-parsed source snapshot.
    pub snapshot: Snapshot,
    /// Whether the caller explicitly accepted stale data.
    pub freshness: Freshness,
}

/// A bounded cache with one replaceable entry per source list.
#[derive(Clone, Debug)]
pub struct FileCache {
    directory: PathBuf,
    limits: ParseLimits,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    version: u32,
    list: List,
    source_url: Option<String>,
    retrieved_at: SystemTime,
    validated_at: SystemTime,
    digest: String,
    etag: Option<String>,
    last_modified: Option<String>,
}

impl FileCache {
    /// Configure a cache directory without touching the filesystem.
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            limits: ParseLimits::default(),
        }
    }

    /// Apply the same input bounds to cache writes and reads.
    pub fn with_limits(mut self, limits: ParseLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Configured directory. It is created on the first store.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Read and validate a cached publication. A missing entry is `Ok(None)`;
    /// corruption, incompatible versions, and disallowed staleness are errors.
    pub fn load(
        &self,
        list: List,
        policy: FreshnessPolicy,
        now: SystemTime,
    ) -> Result<Option<CacheHit>> {
        // An atomic replacement lets readers use a single coherent file without
        // holding the writer lock during parsing.
        let file = match File::open(self.path(list)) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let snapshot = self.read(file, list)?;
        let freshness = snapshot.freshness(policy, now)?;
        Ok(Some(CacheHit {
            snapshot,
            freshness,
        }))
    }

    /// Atomically store a valid snapshot. Failed writes before replacement leave
    /// the old file intact. An older writer cannot replace a newer validation.
    /// Corrupt existing entries may be replaced by a valid snapshot.
    pub fn store(&self, snapshot: &Snapshot) -> Result<()> {
        if snapshot.source_bytes().len() > self.limits.max_bytes {
            return Err(Error::LimitExceeded {
                resource: "cache document bytes",
                limit: self.limits.max_bytes,
            });
        }
        // Respect every configured parser bound even for snapshots created with
        // more permissive settings elsewhere.
        Snapshot::from_xml_with_limits(
            snapshot.metadata.list,
            snapshot.source_bytes(),
            snapshot.metadata.retrieved_at,
            &self.limits,
        )?;
        let header = Header {
            version: 1,
            list: snapshot.metadata.list,
            source_url: snapshot.metadata.source_url.clone(),
            retrieved_at: snapshot.metadata.retrieved_at,
            validated_at: snapshot.metadata.validated_at,
            digest: snapshot.metadata.digest.clone(),
            etag: snapshot.metadata.etag.clone(),
            last_modified: snapshot.metadata.last_modified.clone(),
        };
        let header = serde_json::to_vec(&header)
            .map_err(|_| Error::InvalidCache("metadata cannot be encoded"))?;
        if header.len() > MAX_HEADER_BYTES {
            return Err(Error::InvalidCache("metadata exceeds size limit"));
        }
        fs::create_dir_all(&self.directory)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.directory.join("writer.lock"))?;
        lock.lock()?;
        let path = self.path(snapshot.metadata.list);
        match File::open(&path) {
            Ok(file) => match self.read(file, snapshot.metadata.list) {
                Ok(previous) if previous.metadata.validated_at > snapshot.metadata.validated_at => {
                    return Err(Error::InvalidCache("refusing to replace a newer snapshot"));
                }
                Err(Error::Io(error)) => return Err(error.into()),
                _ => {}
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let mut file = tempfile::NamedTempFile::new_in(&self.directory)?;
        file.write_all(MAGIC)?;
        let header_len = u32::try_from(header.len())
            .map_err(|_| Error::InvalidCache("metadata exceeds size limit"))?;
        file.write_all(&header_len.to_le_bytes())?;
        file.write_all(&header)?;
        file.write_all(snapshot.source_bytes())?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|error| Error::Io(error.error))?;
        #[cfg(unix)]
        File::open(&self.directory)?.sync_all()?;
        // Dropping the lock handle releases the advisory lock on every path.
        Ok(())
    }

    fn path(&self, list: List) -> PathBuf {
        self.directory.join(format!("{}.ofac-cache", list.as_str()))
    }

    fn read(&self, file: File, list: List) -> Result<Snapshot> {
        let max_bytes = self
            .limits
            .max_bytes
            .saturating_add(MAX_HEADER_BYTES + MAGIC.len() + 4);
        let mut bytes = Vec::new();
        file.take((max_bytes as u64).saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() > max_bytes {
            return Err(Error::InvalidCache("entry exceeds size limit"));
        }
        if bytes.get(..8) != Some(MAGIC.as_slice()) {
            return Err(Error::InvalidCache("unknown format or version"));
        }
        let len: [u8; 4] = bytes
            .get(8..12)
            .and_then(|b| b.try_into().ok())
            .ok_or(Error::InvalidCache("truncated header"))?;
        let len = u32::from_le_bytes(len) as usize;
        if len > MAX_HEADER_BYTES {
            return Err(Error::InvalidCache("metadata exceeds size limit"));
        }
        let header: Header = serde_json::from_slice(
            bytes
                .get(12..12 + len)
                .ok_or(Error::InvalidCache("truncated metadata"))?,
        )
        .map_err(|_| Error::InvalidCache("malformed metadata"))?;
        if header.version != 1 || header.list != list {
            return Err(Error::InvalidCache("source or version mismatch"));
        }
        if header
            .source_url
            .as_deref()
            .is_some_and(|url| url != list.url())
        {
            return Err(Error::InvalidCache("source URL mismatch"));
        }
        if header.validated_at < header.retrieved_at {
            return Err(Error::InvalidCache("inconsistent timestamps"));
        }
        let mut snapshot = Snapshot::from_xml_with_limits(
            list,
            &bytes[12 + len..],
            header.retrieved_at,
            &self.limits,
        )?;
        if snapshot.metadata.digest != header.digest {
            return Err(Error::InvalidCache("content digest mismatch"));
        }
        snapshot.metadata.source_url = header.source_url;
        snapshot.metadata.validated_at = header.validated_at;
        snapshot.metadata.etag = header.etag;
        snapshot.metadata.last_modified = header.last_modified;
        Ok(snapshot)
    }
}
