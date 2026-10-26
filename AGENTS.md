# YapSpace: agent instructions

YapSpace = a browser extension + a Rust backend that add a shared real-time discussion to any web page.
Technical id: `yapspace`. Read `docs/architecture/overview.md` for the big picture.

## Start every session like this
1. Read `docs/plan/README.md` (status board + resume protocol) and the last 3 entries of `docs/plan/HANDOFF.md`.
2. Follow the resume protocol exactly. Work on ONE task at a time. Its spec is in `docs/plan/phase-N.md`.
3. Before coding, read every `docs/architecture/*.md` section the task links to. These docs are the source of truth.

## Hard rules
- Commit directly on `develop`. Small conventional commits, lowercase, no period: `feat: added login endpoint`.
- NEVER add `Co-Authored-By`, "Generated with", or any mention of AI/Claude/LLMs to commits, PRs, code or docs.
- Never skip hooks, never force-push, never commit secrets (`.env`, `deploy/secrets/*`).
- Don't change architecture decisions. If a spec is wrong or impossible: mark the task `NEEDS_DECISION`, explain in
  `docs/plan/decisions.md` with a recommended fix, and stop.
- Run `task check` before marking a task DONE. Keep `docs/` in sync with the code (document only what exists).
- Never use `{@html}` in Svelte. Never build SQL from strings. Never log tokens, passwords, IPs, URLs or message bodies.
- Migrations are immutable once committed. Add a new migration instead.
- After changing SQL queries run `task db:prepare` and commit `.sqlx/`. After changing API types run `task api:openapi` and `task ext:types`.

## Commands (Taskfile)
`task setup` · `task dev:db:up` · `task db:migrate` · `task api:run` · `task api:test` · `task api:lint` · `task db:prepare` ·
`task api:openapi` · `task ext:dev` · `task ext:test` · `task ext:build` · `task check` (everything CI runs locally) · `task docker:build`

## Layout
crates/yapspace-server (api → domain → db; realtime/), crates/yapspace-identity (pure URL rules), extension/ (WXT + Svelte 5),
spec/url-identity/vectors.json, deploy/, docs/.
