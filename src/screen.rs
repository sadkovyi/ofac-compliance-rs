use std::{
    collections::{BTreeMap, HashMap, HashSet},
    num::NonZeroUsize,
    time::SystemTime,
};

use crate::{
    ALGORITHM_VERSION, Entry, Error, Freshness, FreshnessPolicy, List, Result, Snapshot,
    SnapshotMetadata, normalize, snapshot::digest,
};

const MAX_QUERY_BYTES: usize = 4096;

/// The operation that produced a name candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatchMethod {
    /// Equality after documented Unicode and punctuation normalization.
    NormalizedExact,
}

/// Per-query name matching policy. Exact matching is the default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NameOptions {
    include_weak: bool,
    limit: Option<NonZeroUsize>,
}

impl Default for NameOptions {
    fn default() -> Self {
        Self::exact()
    }
}

impl NameOptions {
    /// Search the normalized exact-name index. Returns all candidates.
    pub const fn exact() -> Self {
        Self {
            include_weak: true,
            limit: None,
        }
    }

    /// Limit returned entities after scoring and ranking. Reports still contain
    /// the total number of qualifying entities and indicate truncation.
    pub const fn with_limit(mut self, limit: NonZeroUsize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Include or exclude aliases explicitly marked weak. Default: included.
    pub const fn with_weak_aliases(mut self, include: bool) -> Self {
        self.include_weak = include;
        self
    }
}

/// Evidence for one candidate. The original source entry remains available.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Evidence<'a> {
    /// A primary name or an alias matched the query.
    Name {
        /// Original name text.
        matched_name: &'a str,
        /// Alias index in `Entry::aliases`, or `None` for the primary name.
        alias_index: Option<usize>,
        /// Similarity in `[0, 1]`; normalized exact matches have score 1.
        score: f64,
        /// Which comparison produced this score.
        method: MatchMethod,
    },
}

/// One source entity that met the query's matching rules.
#[derive(Clone, Debug)]
pub struct Candidate<'a> {
    /// Publication containing the entity.
    pub list: List,
    /// The original listing; IDs are scoped by `list`.
    pub entry: &'a Entry,
    /// Best matching name or first matching identifier in source order.
    pub evidence: Evidence<'a>,
}

/// Provenance and age for a screened publication.
#[derive(Clone, Debug)]
pub struct SourceStatus<'a> {
    /// The exact source used.
    pub metadata: &'a SnapshotMetadata,
    /// Freshness at the time of this operation.
    pub freshness: Freshness,
}

/// Screening candidates with complete coverage and truncation information.
#[derive(Clone, Debug)]
pub struct Report<'a> {
    /// SHA3-256 identity of the selected source contents and algorithm version.
    pub snapshot_id: &'a str,
    /// Version of normalization and matching semantics.
    pub algorithm_version: &'static str,
    /// Timestamp used to evaluate freshness.
    pub screened_at: SystemTime,
    /// Freshness policy used for this operation.
    pub freshness_policy: FreshnessPolicy,
    /// All publications screened, including any allowed stale publications.
    pub sources: Vec<SourceStatus<'a>>,
    /// Name policy for name searches; absent for identifier searches.
    pub name_options: Option<NameOptions>,
    /// Candidates, ordered by descending score, then list and entity ID.
    pub candidates: Vec<Candidate<'a>>,
    /// Qualifying entity count before applying the result limit.
    pub total_candidates: usize,
}

impl Report<'_> {
    /// Whether the configured limit omitted any qualifying entities.
    pub fn truncated(&self) -> bool {
        self.candidates.len() < self.total_candidates
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Location {
    snapshot: usize,
    entry: usize,
}

#[derive(Debug)]
struct IndexedName {
    location: Location,
    alias: Option<usize>,
    weak: bool,
}

/// Immutable indexes over one or more complete publications. Safe to share
/// between threads; screening performs no network access or filesystem writes.
#[derive(Debug)]
pub struct Screener {
    snapshots: Vec<Snapshot>,
    policy: FreshnessPolicy,
    snapshot_id: String,
    names: Vec<IndexedName>,
    exact: HashMap<String, Vec<usize>>,
}

impl Screener {
    /// Build indexes and verify that every required list is present. All supplied
    /// lists are screened. Duplicate publications and empty requirements fail.
    /// Freshness is evaluated on each query, not frozen at construction time.
    pub fn new(
        snapshots: impl IntoIterator<Item = Snapshot>,
        required_lists: &[List],
        policy: FreshnessPolicy,
    ) -> Result<Self> {
        if required_lists.is_empty() {
            return Err(Error::EmptyCoverage);
        }
        let mut required = HashSet::new();
        for list in required_lists {
            if !required.insert(*list) {
                return Err(Error::DuplicateList(*list));
            }
        }
        let mut snapshots: Vec<_> = snapshots.into_iter().collect();
        snapshots.sort_by_key(|s| s.metadata.list);
        let mut present = HashSet::new();
        for snapshot in &snapshots {
            if !present.insert(snapshot.metadata.list) {
                return Err(Error::DuplicateList(snapshot.metadata.list));
            }
        }
        for list in required_lists {
            if !present.contains(list) {
                return Err(Error::MissingList(*list));
            }
        }
        let mut identity = String::from(ALGORITHM_VERSION);
        let mut names = Vec::new();
        let mut exact: HashMap<String, Vec<usize>> = HashMap::new();
        for (s, snapshot) in snapshots.iter().enumerate() {
            identity.push_str(&format!(
                "\n{}:{}",
                snapshot.metadata.list, snapshot.metadata.digest
            ));
            for (e, entry) in snapshot.entries().iter().enumerate() {
                let location = Location {
                    snapshot: s,
                    entry: e,
                };
                let entry_names = std::iter::once((None, entry.name.as_str(), false)).chain(
                    entry
                        .aliases
                        .iter()
                        .enumerate()
                        .map(|(i, a)| (Some(i), a.name.as_str(), a.is_weak())),
                );
                for (alias, value, weak) in entry_names {
                    let normalized = normalize::name(value);
                    exact
                        .entry(normalized.clone())
                        .or_default()
                        .push(names.len());
                    names.push(IndexedName {
                        location,
                        alias,
                        weak,
                    });
                }
            }
        }
        Ok(Self {
            snapshots,
            policy,
            snapshot_id: digest(identity.as_bytes()),
            names,
            exact,
        })
    }

    /// Stable identity of this dataset and matching algorithm.
    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }

    /// Publications in stable list order.
    pub fn snapshots(&self) -> &[Snapshot] {
        &self.snapshots
    }

    /// Number of distinct source entities, counting each list independently.
    pub fn len(&self) -> usize {
        self.snapshots.iter().map(|s| s.entries().len()).sum()
    }

    /// Whether there are no entities. Successfully built screeners are nonempty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Screen a name using the current system time for freshness.
    pub fn screen_name(&self, query: &str, options: NameOptions) -> Result<Report<'_>> {
        self.screen_name_at(query, options, SystemTime::now())
    }

    /// Screen a name at an explicit time. Empty or oversized queries fail before
    /// matching. Each entity appears once, using its best-scoring name.
    pub fn screen_name_at(
        &self,
        query: &str,
        options: NameOptions,
        now: SystemTime,
    ) -> Result<Report<'_>> {
        check_query(query)?;
        let query = normalize::name(query);
        if !normalize::searchable(&query) {
            return Err(Error::EmptyQuery);
        }
        let sources = self.statuses(now)?;
        let mut best: BTreeMap<Location, (usize, f64, MatchMethod)> = BTreeMap::new();
        if let Some(indices) = self.exact.get(&query) {
            for &index in indices {
                let name = &self.names[index];
                if name.weak && !options.include_weak {
                    continue;
                }
                best.entry(name.location)
                    .or_insert((index, 1.0, MatchMethod::NormalizedExact));
            }
        }
        let mut ranked: Vec<_> = best.into_iter().collect();
        ranked.sort_by(|(a, (_, score_a, _)), (b, (_, score_b, _))| {
            score_b
                .total_cmp(score_a)
                .then_with(|| self.key(*a).cmp(&self.key(*b)))
        });
        let total_candidates = ranked.len();
        if let Some(limit) = options.limit {
            ranked.truncate(limit.get());
        }
        let candidates = ranked
            .into_iter()
            .map(|(location, (index, score, method))| {
                let name = &self.names[index];
                let entry = self.entry(location);
                let matched_name = name
                    .alias
                    .map_or(entry.name.as_str(), |i| entry.aliases[i].name.as_str());
                Candidate {
                    list: self.snapshots[location.snapshot].metadata.list,
                    entry,
                    evidence: Evidence::Name {
                        matched_name,
                        alias_index: name.alias,
                        score,
                        method,
                    },
                }
            })
            .collect();
        Ok(Report {
            snapshot_id: &self.snapshot_id,
            algorithm_version: ALGORITHM_VERSION,
            screened_at: now,
            freshness_policy: self.policy,
            sources,
            name_options: Some(options),
            candidates,
            total_candidates,
        })
    }

    fn entry(&self, location: Location) -> &Entry {
        &self.snapshots[location.snapshot].entries()[location.entry]
    }

    fn key(&self, location: Location) -> (List, &str) {
        (
            self.snapshots[location.snapshot].metadata.list,
            &self.entry(location).uid,
        )
    }

    fn statuses(&self, now: SystemTime) -> Result<Vec<SourceStatus<'_>>> {
        self.snapshots
            .iter()
            .map(|snapshot| {
                Ok(SourceStatus {
                    metadata: snapshot.metadata(),
                    freshness: snapshot.freshness(self.policy, now)?,
                })
            })
            .collect()
    }
}

fn check_query(query: &str) -> Result<()> {
    if query.len() > MAX_QUERY_BYTES {
        return Err(Error::LimitExceeded {
            resource: "query bytes",
            limit: MAX_QUERY_BYTES,
        });
    }
    if query.trim().is_empty() {
        return Err(Error::EmptyQuery);
    }
    Ok(())
}
