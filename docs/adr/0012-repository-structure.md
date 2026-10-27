# 0012 Repository structure

Status: Accepted · Date: 2026-10-27

## Context
Part of the YapSpace MVP architecture (see docs/architecture/overview.md). Decision area: repository structure.

## Decision
A monorepo: crates/ (Rust), extension/ (WXT), spec/ (shared test vectors), deploy/ (ops), docs/ (plans, architecture and ADRs).

## Alternatives considered
- Separate repositories: harder to change the API and the client together.

## Reasons
- One change can update the API, the generated types and the client, in one commit.

## Consequences
- CI has to build both halves.

## Revisit when
- Separate teams or release cadences.
