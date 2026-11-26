# Changelog

## 0.1.0 — Unreleased

- Parse standard SDN and consolidated non-SDN XML with bounded resource use.
- Reject deeply nested XML without overflowing the parser stack.
- Preserve entity IDs, aliases, identifying fields, program tags, and source bytes.
- Screen primary names and aliases with exact or explicit-threshold fuzzy matching.
- Look up identifiers without changing case or punctuation.
- Report matching evidence, coverage, content identity, freshness, and truncation.
- Add optional conditional HTTPS acquisition, disk caching, and Serde support.
