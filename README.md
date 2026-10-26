# YapSpace

> The internet is better when we talk.

YapSpace adds a shared, real-time discussion to any web page through a browser extension, backed by a Rust API.

**Start here:** [AGENTS.md](AGENTS.md) · implementation status: [docs/plan/README.md](docs/plan/README.md) · architecture: [docs/architecture/overview.md](docs/architecture/overview.md)

## Local development

Requirements: Rust (the version in `rust-toolchain.toml`), Docker, and [go-task](https://taskfile.dev).

```bash
task setup          # installs sqlx-cli and cargo-deny
task dev:db:up      # starts PostgreSQL 18 on 127.0.0.1:5432
cp .env.example .env
task db:migrate     # applies migrations
task api:run        # serves on 127.0.0.1:8080, metrics on 127.0.0.1:9090
task check          # everything CI runs
```

Tests that use a database (`#[sqlx::test]`) read `DATABASE_URL` and create a throwaway database for each test, so the
dev database must be running (`task dev:db:up`) and `DATABASE_URL` must be set. `task` loads it from `.env`.
