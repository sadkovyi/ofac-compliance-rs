# Security

For a suspected vulnerability, use the repository's private vulnerability
reporting channel if enabled. Otherwise contact the repository maintainer
privately before opening a public issue. Do not include customer screening
queries, credentials, or private identifiers in reports or reproduction files.

Useful reports include the affected version/features, a minimal synthetic input,
the observed behavior, and the expected boundary. Parser resource exhaustion,
cache integrity failures, and refresh behavior that conceals unavailable or
stale data are within scope.

Source digests detect content changes and accidental corruption. They are not
publisher signatures. Offline bytes and cache directories must come from a
trusted application-controlled source. HTTPS downloads validate certificates
through the HTTP client's TLS implementation.

Only the current development line is maintained; no release support window has
been established yet.
