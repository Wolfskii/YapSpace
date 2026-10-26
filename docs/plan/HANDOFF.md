# Handoff log

Newest entry at the bottom. Template:

```markdown
## YYYY-MM-DD · P1-04 · IN_PROGRESS → DONE
- Did: …
- Commits: <hash> <subject>
- Checks: …
- Next: … Gotcha: …
```

## 2026-10-26 · P0-01 · TODO → DONE
- Did: plan split into `docs/architecture/`, `docs/adr/README.md`, `docs/plan/` (board + phase files), `AGENTS.md`, `CLAUDE.md`, `README.md`, `.gitignore`, `.gitattributes`, `.editorconfig`.
- Not yet: ADR files (P0-03) and URL vectors (P0-04) are still TODO.
- Next: P1-01 (or P0-03 / P0-04 in parallel).

## 2026-10-26 · P1-01 · TODO → DONE
- Did: `.env.example` (dev vars, no secrets). Local `.env` copied (gitignored, verified with `git check-ignore`).
- Commits: see `git log --oneline -1`. Message: chore: added dev env example
- Checks: `git check-ignore -v .env` matches `.gitignore`.
- Next: P1-02 (Rust workspace skeleton + CLI).

## 2026-10-26 · P1-02 · TODO → DONE
- Did: root Cargo.toml (workspace, edition 2024, lints), rust-toolchain.toml (1.98), rustfmt.toml, deny.toml; crates/yapspace-identity (lib stub, RULESET=1); crates/yapspace-server (lib with empty module tree per §C.1, bin `yapspace` with clap subcommands serve/migrate/openapi/healthcheck/admin set-role that print 'not implemented yet' and exit 1). Dependencies from §N.2 added, with the crate versions in the plan. Cargo.lock committed.
- Checks: `cargo build --workspace` ok; `cargo fmt --all --check` ok; `cargo clippy --workspace --all-targets -- -D warnings` ok; `yapspace --help` lists the 5 subcommands.
- Gap: `cargo deny check` not run yet (cargo-deny not installed). Install in P1-03 (`task setup`) and run it there.
- Next: P1-03 (dev Postgres compose + Taskfile).
