//! Explicit, bounded HTTPS acquisition of standard OFAC publications.
//!
//! Network access happens only when explicitly requested. Async applications
//! should run acquisition on their runtime's blocking pool. Screening itself
//! needs no runtime, HTTP client, or cache.

use std::{
    io::Read,
    time::{Duration, SystemTime},
};

use crate::{Error, List, ParseLimits, Result, Snapshot};

/// Timeouts and parser limits for a single network request.
#[derive(Clone, Debug)]
pub struct HttpOptions {
    /// Overall timeout for each request, including its response body.
    pub timeout: Duration,
    /// Connection timeout, also bounded by the overall timeout.
    pub connect_timeout: Duration,
    /// Maximum uncompressed download and parser resource bounds.
    pub parse_limits: ParseLimits,
}

impl Default for HttpOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(5),
            parse_limits: ParseLimits::default(),
        }
    }
}

/// Reusable client for the two supported OFAC XML publications.
pub struct HttpClient {
    agent: ureq::Agent,
    options: HttpOptions,
}

struct Response {
    status: u16,
    etag: Option<String>,
    last_modified: Option<String>,
    content_length: Option<u64>,
    body: Box<dyn Read + Send + Sync>,
}

impl HttpClient {
    /// Construct a TLS client with explicit timeouts.
    pub fn new(options: HttpOptions) -> Result<Self> {
        if options.timeout.is_zero() || options.connect_timeout.is_zero() {
            return Err(Error::InvalidConfiguration("HTTP timeouts must be nonzero"));
        }
        let agent = ureq::AgentBuilder::new()
            .https_only(true)
            .redirects(3)
            .timeout(options.timeout)
            .timeout_connect(options.connect_timeout)
            .user_agent(concat!("ofac-compliance/", env!("CARGO_PKG_VERSION")))
            .build();
        Ok(Self { agent, options })
    }

    /// Download and validate a complete publication. No automatic retries occur.
    pub fn download(&self, list: List) -> Result<Snapshot> {
        self.download_using(list, SystemTime::now(), || {
            let request = self
                .agent
                .get(list.url())
                .set("Accept", "application/xml")
                .set("Accept-Encoding", "identity");
            let response = match request.call() {
                Ok(response) | Err(ureq::Error::Status(_, response)) => response,
                Err(ureq::Error::Transport(_)) => return Err(Error::Transport),
            };
            // Content encodings are disabled to bound the bytes we parse and
            // hash. A server ignoring identity is rejected, not misinterpreted.
            if response
                .header("Content-Encoding")
                .is_some_and(|s| !s.eq_ignore_ascii_case("identity"))
            {
                return Err(Error::InvalidDocument("unexpected HTTP content encoding"));
            }
            let result = Response {
                status: response.status(),
                etag: header(&response, "ETag")?,
                last_modified: header(&response, "Last-Modified")?,
                content_length: response
                    .header("Content-Length")
                    .and_then(|s| s.parse().ok()),
                body: response.into_reader(),
            };
            Ok(result)
        })
    }

    fn download_using(
        &self,
        list: List,
        now: SystemTime,
        fetch: impl FnOnce() -> Result<Response>,
    ) -> Result<Snapshot> {
        let response = fetch()?;
        if response.status == 200 {
            let max_bytes = self.options.parse_limits.max_bytes;
            if response
                .content_length
                .is_some_and(|len| len > max_bytes as u64)
            {
                return Err(Error::LimitExceeded {
                    resource: "download bytes",
                    limit: max_bytes,
                });
            }
            let mut bytes = Vec::new();
            response
                .body
                .take((max_bytes as u64).saturating_add(1))
                .read_to_end(&mut bytes)?;
            if bytes.len() > max_bytes {
                return Err(Error::LimitExceeded {
                    resource: "download bytes",
                    limit: max_bytes,
                });
            }
            if response
                .content_length
                .is_some_and(|len| len != bytes.len() as u64)
            {
                return Err(Error::InvalidDocument("HTTP body length mismatch"));
            }
            let mut snapshot =
                Snapshot::from_xml_with_limits(list, &bytes, now, &self.options.parse_limits)?;
            snapshot.metadata.source_url = Some(list.url().to_owned());
            snapshot.metadata.etag = response.etag;
            snapshot.metadata.last_modified = response.last_modified;
            return Ok(snapshot);
        }
        Err(Error::HttpStatus(response.status))
    }
}

fn header(response: &ureq::Response, key: &str) -> Result<Option<String>> {
    match response.header(key) {
        Some(value) if value.len() > 4096 => Err(Error::LimitExceeded {
            resource: "HTTP header bytes",
            limit: 4096,
        }),
        value => Ok(value.map(str::to_owned)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Cursor, time::UNIX_EPOCH};
    const XML: &[u8] = include_bytes!("../tests/fixtures/sdn.xml");
    fn response(status: u16, bytes: &[u8]) -> Response {
        Response {
            status,
            etag: Some("\"v1\"".into()),
            last_modified: None,
            content_length: Some(bytes.len() as u64),
            body: Box::new(Cursor::new(bytes.to_vec())),
        }
    }
    #[test]
    fn downloads_only_complete_valid_publications() {
        let client = HttpClient::new(HttpOptions::default()).unwrap();
        let snapshot = client
            .download_using(List::Sdn, UNIX_EPOCH, || Ok(response(200, XML)))
            .unwrap();
        assert_eq!(snapshot.entries().len(), 4);
        assert_eq!(
            snapshot.metadata.source_url.as_deref(),
            Some(List::Sdn.url())
        );
        for status in [204, 206, 304, 404, 503] {
            assert!(
                client
                    .download_using(List::Sdn, UNIX_EPOCH, || Ok(response(status, XML)))
                    .is_err()
            );
        }
    }
    #[test]
    fn bounds_downloads_even_without_content_length() {
        let client = HttpClient::new(HttpOptions {
            parse_limits: ParseLimits {
                max_bytes: 16,
                ..ParseLimits::default()
            },
            ..HttpOptions::default()
        })
        .unwrap();
        let result = client.download_using(List::Sdn, UNIX_EPOCH, || {
            let mut r = response(200, XML);
            r.content_length = None;
            Ok(r)
        });
        assert!(matches!(result, Err(Error::LimitExceeded { .. })));
    }
}
