# 0008 Authentication

Status: Accepted · Date: 2026-10-27

## Context
Part of the YapSpace MVP architecture (see docs/architecture/overview.md). Decision area: authentication.

## Decision
Username and password (argon2id), opaque session tokens (only SHA-256 stored), and a one-time recovery code. No email in the MVP.

## Alternatives considered
- Email magic links: needs a mail provider and an email address for every account.
- External identity providers: vendor coupling and browser redirect differences.
- JWT: revocation is hard.

## Reasons
- Sessions are revocable, and each one can be listed and removed.
- Timing is equalized so unknown users look like wrong passwords.

## Consequences
- Losing both the password and the recovery code means losing the account.
- No second factor yet.

## Revisit when
- Email becomes a requirement, or an OIDC login is requested.
