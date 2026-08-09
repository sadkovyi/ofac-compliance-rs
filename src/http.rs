//! Blocking HTTPS acquisition with bounded retries and conditional requests.
//!
//! Network access happens only when explicitly requested. Async applications
//! should run acquisition on their runtime's blocking pool. Screening itself
//! needs no runtime, HTTP client, or cache.

use std::{
    io::Read,
    time::{Duration, SystemTime},
};

use crate::{Error, List, ParseLimits, Result, Snapshot};

/// Limits for network acquisition. Defaults allow up to three attempts.
#[derive(Clone, Debug)]
pub struct HttpOptions {
    /// Overall timeout for each request, including its response body.
    pub timeout: Duration,
    /// Connection timeout, also bounded by the overall timeout.
    pub connect_timeout: Duration,
    /// Retries after the first attempt, at most five. Only transient HTTP statuses
    /// are retried; transport failures and parse failures are returned directly.
    pub retries: u8,
    /// Initial exponential retry delay.
    pub retry_delay: Duration,
    /// Largest permitted delay. Longer Retry-After values end the operation.
    pub max_retry_delay: Duration,
    /// Maximum uncompressed download and parser resource bounds.
    pub parse_limits: ParseLimits,
}

impl Default for HttpOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(5),
            retries: 2,
            retry_delay: Duration::from_millis(250),
            max_retry_delay: Duration::from_secs(30),
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
    retry_after: Option<String>,
    content_length: Option<u64>,
    body: Box<dyn Read + Send + Sync>,
}

impl HttpClient {
    /// Construct a TLS client with explicit timeout and retry bounds.
    pub fn new(options: HttpOptions) -> Result<Self> {
        if options.timeout.is_zero()
            || options.connect_timeout.is_zero()
            || options.retries > 5
            || options.retry_delay > options.max_retry_delay
        {
            return Err(Error::InvalidConfiguration(
                "invalid HTTP timeout or retry bounds",
            ));
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

    /// Fetch and validate a complete publication. A previous snapshot enables
    /// conditional requests when its provenance and validators match this list.
    /// On any failure, the borrowed previous snapshot remains untouched.
    pub fn refresh(&self, list: List, previous: Option<&Snapshot>) -> Result<Snapshot> {
        self.refresh_using(
            list,
            previous,
            SystemTime::now(),
            |etag, modified| {
                let mut request = self
                    .agent
                    .get(list.url())
                    .set("Accept", "application/xml")
                    .set("Accept-Encoding", "identity");
                if let Some(etag) = etag {
                    request = request.set("If-None-Match", etag);
                }
                if let Some(modified) = modified {
                    request = request.set("If-Modified-Since", modified);
                }
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
                    retry_after: header(&response, "Retry-After")?,
                    content_length: response
                        .header("Content-Length")
                        .and_then(|s| s.parse().ok()),
                    body: response.into_reader(),
                };
                Ok(result)
            },
            std::thread::sleep,
        )
    }

    fn refresh_using(
        &self,
        list: List,
        previous: Option<&Snapshot>,
        now: SystemTime,
        mut fetch: impl FnMut(Option<&str>, Option<&str>) -> Result<Response>,
        mut sleep: impl FnMut(Duration),
    ) -> Result<Snapshot> {
        if previous.is_some_and(|s| s.metadata.list != list) {
            return Err(Error::InvalidConfiguration(
                "previous snapshot is for another list",
            ));
        }
        if previous.is_some_and(|s| s.metadata.validated_at > now) {
            return Err(Error::ClockSkew(list));
        }
        let conditional = previous.filter(|s| s.metadata.source_url.as_deref() == Some(list.url()));
        let etag = conditional.and_then(|s| s.metadata.etag.as_deref());
        let modified = conditional.and_then(|s| s.metadata.last_modified.as_deref());
        for attempt in 0..=self.options.retries {
            let response = fetch(etag, modified)?;
            if response.status == 304 {
                let mut snapshot = conditional
                    .filter(|_| etag.is_some() || modified.is_some())
                    .ok_or(Error::UnexpectedNotModified)?
                    .clone();
                snapshot.metadata.validated_at = now;
                if response.etag.is_some() {
                    snapshot.metadata.etag = response.etag;
                }
                if response.last_modified.is_some() {
                    snapshot.metadata.last_modified = response.last_modified;
                }
                return Ok(snapshot);
            }
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
            if attempt < self.options.retries
                && matches!(response.status, 408 | 429 | 500 | 502 | 503 | 504)
            {
                let delay = match response.retry_after.as_deref() {
                    // HTTP-date values are returned to the caller instead of
                    // guessing a delay or retrying before the requested time.
                    Some(value) => match value.parse::<u64>() {
                        Ok(seconds) => Duration::from_secs(seconds),
                        Err(_) => return Err(Error::HttpStatus(response.status)),
                    },
                    None => self.options.retry_delay.saturating_mul(1 << attempt),
                };
                if delay > self.options.max_retry_delay {
                    return Err(Error::HttpStatus(response.status));
                }
                drop(response);
                sleep(delay);
            } else {
                return Err(Error::HttpStatus(response.status));
            }
        }
        Err(Error::Transport)
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
    use std::{cell::Cell, io::Cursor, time::UNIX_EPOCH};

    const XML: &[u8] = include_bytes!("../tests/fixtures/sdn.xml");

    fn response(status: u16, bytes: &[u8]) -> Response {
        Response {
            status,
            etag: Some("\"v1\"".into()),
            last_modified: None,
            retry_after: None,
            content_length: Some(bytes.len() as u64),
            body: Box::new(Cursor::new(bytes.to_vec())),
        }
    }

    fn client() -> HttpClient {
        HttpClient::new(HttpOptions::default()).unwrap()
    }

    #[test]
    fn conditional_refresh_keeps_content_and_retrieval_time() {
        let client = client();
        let original = client
            .refresh_using(
                List::Sdn,
                None,
                UNIX_EPOCH,
                |etag, _| {
                    assert!(etag.is_none());
                    Ok(response(200, XML))
                },
                |_| panic!(),
            )
            .unwrap();
        let now = UNIX_EPOCH + Duration::from_secs(60);
        let fresh = client
            .refresh_using(
                List::Sdn,
                Some(&original),
                now,
                |etag, _| {
                    assert_eq!(etag, Some("\"v1\""));
                    Ok(response(304, b""))
                },
                |_| panic!(),
            )
            .unwrap();
        assert_eq!(fresh.metadata.digest, original.metadata.digest);
        assert_eq!(fresh.metadata.retrieved_at, UNIX_EPOCH);
        assert_eq!(fresh.metadata.validated_at, now);
        assert_eq!(original.metadata.validated_at, UNIX_EPOCH);
    }

    #[test]
    fn retry_budget_and_failed_refresh_do_not_change_previous() {
        let client = client();
        let previous = Snapshot::from_xml(List::Sdn, XML, UNIX_EPOCH).unwrap();
        let calls = Cell::new(0);
        let sleeps = Cell::new(0);
        let result = client.refresh_using(
            List::Sdn,
            Some(&previous),
            UNIX_EPOCH,
            |_, _| {
                calls.set(calls.get() + 1);
                Ok(response(503, b""))
            },
            |_| sleeps.set(sleeps.get() + 1),
        );
        assert!(matches!(result, Err(Error::HttpStatus(503))));
        assert_eq!(calls.get(), 3);
        assert_eq!(sleeps.get(), 2);
        assert_eq!(previous.source_bytes(), XML);
    }

    #[test]
    fn rejects_unsolicited_304_and_partial_success() {
        for status in [304, 206, 204] {
            assert!(
                client()
                    .refresh_using(
                        List::Sdn,
                        None,
                        UNIX_EPOCH,
                        |_, _| Ok(response(status, XML)),
                        |_| panic!()
                    )
                    .is_err()
            );
        }
    }

    #[test]
    fn parse_failures_and_permanent_statuses_are_not_retried() {
        for (status, bytes) in [(200, b"<broken>".as_slice()), (404, b"".as_slice())] {
            let calls = Cell::new(0);
            let result = client().refresh_using(
                List::Sdn,
                None,
                UNIX_EPOCH,
                |_, _| {
                    calls.set(calls.get() + 1);
                    Ok(response(status, bytes))
                },
                |_| panic!(),
            );
            assert!(result.is_err());
            assert_eq!(calls.get(), 1);
        }
    }

    #[test]
    fn retry_after_is_respected_or_returned_to_caller() {
        for retry_after in ["90", "Wed, 21 Oct 2015 07:28:00 GMT"] {
            let result = client().refresh_using(
                List::Sdn,
                None,
                UNIX_EPOCH,
                |_, _| {
                    let mut response = response(429, b"");
                    response.retry_after = Some(retry_after.into());
                    Ok(response)
                },
                |_| panic!("must not retry early"),
            );
            assert!(matches!(result, Err(Error::HttpStatus(429))));
        }
        let calls = Cell::new(0);
        let fresh = client()
            .refresh_using(
                List::Sdn,
                None,
                UNIX_EPOCH,
                |_, _| {
                    calls.set(calls.get() + 1);
                    if calls.get() == 1 {
                        let mut response = response(429, b"");
                        response.retry_after = Some("2".into());
                        Ok(response)
                    } else {
                        Ok(response(200, XML))
                    }
                },
                |delay| assert_eq!(delay, Duration::from_secs(2)),
            )
            .unwrap();
        assert_eq!(fresh.entries().len(), 4);
    }

    #[test]
    fn bounds_body_even_without_content_length() {
        let client = HttpClient::new(HttpOptions {
            parse_limits: ParseLimits {
                max_bytes: 16,
                ..ParseLimits::default()
            },
            ..HttpOptions::default()
        })
        .unwrap();
        let result = client.refresh_using(
            List::Sdn,
            None,
            UNIX_EPOCH,
            |_, _| {
                let mut response = response(200, XML);
                response.content_length = None;
                Ok(response)
            },
            |_| panic!(),
        );
        assert!(matches!(result, Err(Error::LimitExceeded { .. })));
    }
}
