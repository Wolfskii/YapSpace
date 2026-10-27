# Decisions log

Each entry: status `NEEDS_DECISION` or `DECIDED`, the recommendation, and what it blocks.

| ID | Question | Recommendation | Status | Blocks |
|----|----------|----------------|--------|--------|
| D-01 | Production domain name | placeholder `api.yapspace.example` until chosen | NEEDS_DECISION | P5-09, P5-14, store submission |
| D-02 | VPS provider/region + off-site backup provider | EU VPS, S3-compatible backup storage | NEEDS_DECISION | P5-09, P5-10 |
| D-03 | GitHub remote for CI and GHCR | Wolfskii/YapSpace, public (user choice) | DECIDED | none |
| D-04 | Message content after account deletion | erase bodies, keep tombstones (§H.5). Implemented in the erasure job | DECIDED | none |
| D-05 | Legal entity, privacy policy, terms | specialist review before public launch | NEEDS_DECISION | P5-14, launch |
| D-06 | Moderation policy + EU DSA notice-and-action | legal review | NEEDS_DECISION | P5-03 policy text, launch |
| D-07 | Repo license | proprietary, no LICENSE file | NEEDS_DECISION | none |
