# Contributing

Use the toolchain in `rust-toolchain.toml`. The minimum supported Rust version is
1.89; changes that raise it should be intentional and called out in the changelog.

Before submitting a change:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo test --no-default-features
cargo test --features serde
cargo test --features cache
cargo test --features http
cargo test --all-features
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features
```

Ordinary tests must work without a network connection after dependencies have
been fetched. Keep fixtures synthetic and small. Parser changes should include
a malformed-input regression case as well as the supported case. Matching
changes should show which candidates or scores change, including false positives
and false negatives. Bump `ALGORITHM_VERSION` when normalization, scoring, or
ranking semantics change.

Use the benchmark harness before and after changing index layout or matching.
Report dataset size, feature set, compiler, hardware, and workload. A timing
improvement is not sufficient if it drops previously qualifying candidates.

To explicitly test the live endpoints:

```sh
cargo test --features http --test live -- --ignored
```

That command downloads both publications and may fail because of upstream
availability or format changes. It is deliberately separate from CI.

Keep public API changes focused. Explain the caller-visible behavior in the
change description, add a changelog entry, and update examples where necessary.
Contributions are licensed under the same MIT OR Apache-2.0 terms as the project.
