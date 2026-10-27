# URL identity test vectors

`vectors.json` is the shared conformance suite for `crates/yapspace-identity` (docs/architecture/url-identity.md §E.9).
The suite runs in `crates/yapspace-identity/tests/vectors.rs`. Any other implementation (for example the extension) must
pass the same vectors.

Format: each entry has `name` (unique), `observed`, an optional `canonical`, and `expect`. `expect` may contain `key`
(the first candidate's key), `display_url`, `candidates` (every key, in order), or `error` (a reason string from §F.1).
Assert only the fields that are present.

Rules for editing:
- A released vector is never edited. To change an expectation, add a new vector under the new ruleset.
- Every expectation is worked out by hand from the rules in §E. If a rule seems ambiguous, log it in
  `docs/plan/decisions.md` rather than guessing.
