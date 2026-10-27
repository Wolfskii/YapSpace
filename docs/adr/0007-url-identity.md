# 0007 URL identity

Status: Provisional · Date: 2026-10-27

## Context
Part of the YapSpace MVP architecture (see docs/architecture/overview.md). Decision area: url identity.

## Decision
Versioned, deterministic identity keys built on the server. Site adapters for permalinks. No implicit merging. Admin-only merges. Shared test vectors (90 at the time of writing).

## Alternatives considered
- Strip all query parameters: merges unrelated posts.
- Trust the page canonical blindly: lets a page attach itself to another site's discussion.
- Let the client decide identity: the rules would then exist in two places.

## Reasons
- Deterministic rules can be tested and explained.
- Canonical links count only from the same registrable domain.

## Consequences
- Some sites will split or merge discussions until adapters exist.
- Changing a rule needs a ruleset bump.

## Revisit when
- The wrong-discussion report rate exceeds 1 percent of discussions (P6-01).
