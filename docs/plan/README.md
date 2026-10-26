# YapSpace implementation board

The **only** place for task status. Task specs live in `docs/plan/phase-N.md`; architecture specs in `docs/architecture/`.

## Resume protocol

### 1.2 Status vocabulary (exact tokens; grep-able)

| Token | Meaning | Who sets it |
|---|---|---|
| `TODO` | Not started. All work starts here. | plan |
| `IN_PROGRESS` | An agent has started. The newest `HANDOFF.md` entry describes where it stopped. | agent at start |
| `IN_REVIEW` | Code is done and automated checks pass. A manual check is still needed (e.g. load the extension in a browser, or the user tries it). | agent |
| `BLOCKED` | Can't continue. The board Notes column says **why** and **what unblocks it**. | agent |
| `NEEDS_DECISION` | Needs a product or user choice. The question is in `docs/plan/decisions.md`. | agent |
| `DONE` | Every acceptance criterion is met, `task check` passes, and the work is committed. | agent |
| `REOPENED` | Was `DONE`, but a regression or a gap was found. Notes say what. Treat it like `IN_PROGRESS`. | agent |
| `DEFERRED` | Moved to a later phase on purpose. Notes say which phase. | agent/user |
| `SKIPPED` | Will not be done. Notes give the reason. | user only |

Inside each task spec (phase files), the steps are checkboxes. They track progress *within* the task that is `IN_PROGRESS`:
`- [ ]` not done · `- [x]` done · `- [~]` partly done (add a short note after it) · `- [!]` failed or blocked (add a note).

### 1.3 Status board format (`docs/plan/README.md`)

```markdown
| ID | Title | Pri | Deps | Status | Updated | Notes |
|----|-------|-----|------|--------|---------|-------|
| P1-02 | Rust workspace skeleton | Must | P1-01 | TODO | | |
```
Priorities: `Must` (MVP blocker) · `Should` (MVP, can slip to the end of its phase) · `Could` (nice to have).
The board is the **only** place for task-level status. The phase files hold only specs and step checkboxes.
Each phase section of the board ends with a line: `Phase gate: OPEN | PASSED (date)`.

### 1.4 Resume protocol (goes into `docs/plan/README.md`, `AGENTS.md`, and the `next-task` skill)

1. Read `AGENTS.md`, then `docs/plan/README.md` (the board), then the **last 3 entries** of `docs/plan/HANDOFF.md`.
2. Run `git status` and `git log --oneline -10`. If the tree is dirty and the last handoff says
   `IN_PROGRESS`, continue that work. If the tree is dirty with no explanation, stop and ask the user.
3. Pick the task:
   a. If any task is `IN_PROGRESS` or `REOPENED`, take that one (only one may exist at a time).
   b. Otherwise take the first `TODO` task, in board order, whose `Deps` are all `DONE` (or `DEFERRED`/`SKIPPED`)
      and whose phase's previous phase gate is `PASSED`. There are two exceptions: P0 tasks marked "parallel-OK", and tasks that the phase's
      **Prerequisites/Parallelizable** line explicitly lets start early (e.g. P4-01..P4-03 after P3-01).
   c. If no task qualifies, report the blockers to the user and stop.
4. Set the task to `IN_PROGRESS` with today's date and commit this together with your first code commit (don't commit status-only changes).
5. Read the task spec in `docs/plan/phase-N.md` **and every architecture doc it links to**. Follow the steps in order and
   tick the checkboxes as you go.
6. Before marking `DONE`, run `task check` (or the narrower checks the task lists) and confirm every acceptance criterion.
   If a criterion needs a human (browser UI, real VPS), set `IN_REVIEW` and say in Notes what has to be checked.
7. Update the board and the checkboxes, append a `HANDOFF.md` entry, and commit.
8. If you are stopped mid-task, (context running low, or a failure you can't fix): leave it `IN_PROGRESS` or `BLOCKED`, tick
   what's done, write a precise handoff ("next step: X in file Y; the failing test is Z"), and commit the work in progress
   when it compiles. If it doesn't compile, describe the uncompiled state in the handoff and commit only the docs.
9. **Never change a decision in `docs/architecture/` or `docs/adr/` on your own.** If a spec looks wrong or impossible,
   set `NEEDS_DECISION`, write the problem plus your recommended fix in `docs/plan/decisions.md`, and stop.
10. A phase gate becomes `PASSED` when every `Must` task in the phase is `DONE` and the exit criteria in the phase file hold.

### 1.5 `HANDOFF.md` entry template (newest at the bottom)

```markdown

### 1.6 Commit rules (also in AGENTS.md)

- Commit straight to `develop`. Never force-push. Never skip hooks.
- Messages are short and lowercase after the type, with no period: `feat: added message pagination`, `fix: handled lagged ws receivers`,
  `test: added seq ordering race test`, `docs: marked p2-15 done`, `chore: added taskfile`. Put the task ID at the end if you like:
  `feat: added register endpoint (p2-04)`.
- **No `Co-Authored-By`, no "Generated with", and no mention of AI/LLM/Claude** in commits, PRs or docs.
- One task can have several commits, and every commit must build. Status changes go in the same commit as the work.

## Status tokens
`TODO` · `IN_PROGRESS` · `IN_REVIEW` (needs a manual check) · `BLOCKED` · `NEEDS_DECISION` · `DONE` · `REOPENED` · `DEFERRED` · `SKIPPED` (user only).
Priorities: `Must` · `Should` · `Could`.

## Section map
Section letters in the specs (§A…§S) refer to the original plan. Where each lives:

| § | Topic | File |
|---|---|---|
| A, B, C, N | Overview, assumptions, diagram, repo layout | [architecture/overview.md](../architecture/overview.md) |
| D | Data model | [architecture/data-model.md](../architecture/data-model.md) |
| E | URL identity | [architecture/url-identity.md](../architecture/url-identity.md) |
| F | API | [architecture/api.md](../architecture/api.md) |
| G | Real-time | [architecture/realtime.md](../architecture/realtime.md) |
| H | Security & privacy | [architecture/security.md](../architecture/security.md) |
| I | Extension | [architecture/extension.md](../architecture/extension.md) |
| J | Deployment | [architecture/deployment.md](../architecture/deployment.md) |
| K | Observability | [architecture/observability.md](../architecture/observability.md) |
| L | Capacity & scaling triggers | [architecture/capacity.md](../architecture/capacity.md) |
| M | Testing & CI/CD | [architecture/testing-ci.md](../architecture/testing-ci.md) |
| O | ADR summaries | [adr/README.md](../adr/README.md) |
| P | Risks | [architecture/risks.md](../architecture/risks.md) |
| Q | Backlog | this board + `phase-N.md` |
| R | First ten tasks | P0-01 (done) → P1-01 → P1-02 → P1-03 → P1-04 → P1-05 → P1-06 → P1-07 → P1-08 → P1-09 |
| S | MVP Definition of Done | [phase-5.md](phase-5.md) (P5-16) |

## Open decisions
See [decisions.md](decisions.md). D-01 to D-07 do not block Phases 1–4.

## Board

### Phase 0

| ID | Title | Pri | Deps | Status | Updated | Notes |
|----|-------|-----|------|--------|---------|-------|
| P0-01 | Save the plan into the repo | Must | — | DONE | 2026-10-26 |  |
| P0-02 | Architecture diagrams check | Should | P0-01 | TODO |  | parallel-OK |
| P0-03 | Write ADRs 0001–0015 | Must | P0-01 | TODO |  | parallel-OK |
| P0-04 | URL identity test vectors | Must | P0-01 | TODO |  | parallel-OK |
| P0-05 | Decisions log | Must | P0-01 | DONE | 2026-10-26 |  |

Phase gate: OPEN


### Phase 1

| ID | Title | Pri | Deps | Status | Updated | Notes |
|----|-------|-----|------|--------|---------|-------|
| P1-01 | Repository scaffolding | Must | P0-01 | DONE | 2026-10-26 |  |
| P1-02 | Rust workspace skeleton | Must | P1-01 | DONE | 2026-10-26 |  |
| P1-03 | Dev Postgres + Taskfile | Must | P1-02 | TODO |  |  |
| P1-04 | Configuration module | Must | P1-02 | TODO |  |  |
| P1-05 | Telemetry: logging + metrics | Must | P1-04 | TODO |  |  |
| P1-06 | App skeleton: state, router, errors, health, shutdown | Must | P1-05 | TODO |  |  |
| P1-07 | Database pool + migrations + initial schema | Must | P1-03, P1-06 | TODO |  |  |
| P1-08 | Integration test harness | Must | P1-07 | TODO |  |  |
| P1-09 | Dockerfile + image healthcheck | Must | P1-07 | TODO |  |  |
| P1-10 | CI workflow | Must | P1-08, P1-09 | TODO |  |  |
| P1-11 | OpenAPI skeleton + docs UI | Must | P1-06 | TODO |  |  |

Phase gate: OPEN


### Phase 2

| ID | Title | Pri | Deps | Status | Updated | Notes |
|----|-------|-----|------|--------|---------|-------|
| P2-01 | Validation helpers | Must | P1-08 | TODO |  |  |
| P2-02 | Password hashing & tokens | Must | P2-01 | TODO |  |  |
| P2-03 | Sessions + auth extractors | Must | P2-02 | TODO |  |  |
| P2-04 | Register | Must | P2-03 | TODO |  |  |
| P2-05 | Login / logout | Must | P2-04 | TODO |  |  |
| P2-06 | Me endpoints | Must | P2-05 | TODO |  |  |
| P2-07 | Account recovery | Must | P2-06 | TODO |  |  |
| P2-08 | Identity crate: generic normalization | Must | P0-04, P1-02 | TODO |  |  |
| P2-09 | Identity crate: privacy rules completeness | Must | P2-08 | TODO |  |  |
| P2-10 | Identity crate: site adapters | Must | P2-08 | TODO |  |  |
| P2-11 | Identity crate: resolution candidates | Must | P2-09, P2-10 | TODO |  |  |
| P2-12 | Resolve endpoint | Must | P2-11, P2-03 | TODO |  |  |
| P2-13 | Create discussion | Must | P2-12 | TODO |  |  |
| P2-14 | Get discussion | Must | P2-13 | TODO |  |  |
| P2-15 | Post message | Must | P2-14 | TODO |  |  |
| P2-16 | List messages (pagination) | Must | P2-15 | TODO |  |  |
| P2-17 | Single message + author delete | Must | P2-16 | TODO |  |  |
| P2-18 | Rate limiting + client IP | Must | P2-17 | TODO |  |  |
| P2-19 | Duplicate-message guard | Should | P2-18 | TODO |  |  |
| P2-20 | Admin CLI | Must | P2-06 | TODO |  |  |
| P2-21 | OpenAPI completeness + API doc | Must | P2-17 | TODO |  |  |
| P2-22 | Authorization matrix + hardening tests | Must | P2-21 | TODO |  |  |

Phase gate: OPEN


### Phase 3

| ID | Title | Pri | Deps | Status | Updated | Notes |
|----|-------|-----|------|--------|---------|-------|
| P3-01 | Protocol types | Must | P2-22 | TODO |  |  |
| P3-02 | Hub | Must | P3-01 | TODO |  |  |
| P3-03 | WebSocket endpoint & connection task | Must | P3-02, P2-18 | TODO |  |  |
| P3-04 | Subscribe with gap-free catch-up | Must | P3-03 | TODO |  |  |
| P3-05 | Publish from the domain | Must | P3-04 | TODO |  |  |
| P3-06 | Graceful shutdown for sockets | Must | P3-05 | TODO |  |  |
| P3-07 | Authenticated hello + per-user connection cap | Should | P3-03 | TODO |  |  |
| P3-08 | Realtime robustness test suite | Must | P3-06 | TODO |  |  |
| P3-09 | Load scripts + first measurements | Should | P3-08 | TODO |  |  |

Phase gate: OPEN


### Phase 4

| ID | Title | Pri | Deps | Status | Updated | Notes |
|----|-------|-----|------|--------|---------|-------|
| P4-01 | Scaffold WXT + Svelte | Must | P1-10 | TODO |  |  |
| P4-02 | Spike: injection model on real sites (validates ADR 0011) | Must | P4-01 | TODO |  |  |
| P4-03 | Page context pipeline | Must | P4-02 | TODO |  |  |
| P4-04 | API client + generated types | Must | P4-01, P2-21 | TODO |  |  |
| P4-05 | Session store + auth UI | Must | P4-04 | TODO |  |  |
| P4-06 | Discussion store (state machine + pagination) | Must | P4-05, P4-03 | TODO |  |  |
| P4-07 | Message list UI | Must | P4-06 | TODO |  |  |
| P4-08 | Composer (post, reply, first post creates the discussion) | Must | P4-07 | TODO |  |  |
| P4-09 | Realtime client integration | Must | P4-08, P3-05 | TODO |  |  |
| P4-10 | Report + delete own message (client) | Must | P4-09 | TODO |  |  |
| P4-11 | States, a11y, i18n polish | Must | P4-09 | TODO |  |  |
| P4-12 | Cross-browser validation + store packaging docs | Must | P4-11 | TODO |  |  |
| P4-13 | Playwright e2e (Chromium) | Should | P4-11 | TODO |  |  |

Phase gate: OPEN


### Phase 5

| ID | Title | Pri | Deps | Status | Updated | Notes |
|----|-------|-----|------|--------|---------|-------|
| P5-01 | Threat model review | Must | — (start of phase) | TODO |  |  |
| P5-02 | Reports API | Must | P2-22 | TODO |  |  |
| P5-03 | Moderation API + enforcement | Must | P5-02 | TODO |  |  |
| P5-04 | Moderation UI (extension) | Should | P5-03 | TODO |  |  |
| P5-05 | Blocks | Should | P5-02 | TODO |  |  |
| P5-06 | Account deletion + export | Must | P5-03 | TODO |  |  |
| P5-07 | Background jobs | Must | P5-06 | TODO |  |  |
| P5-08 | Hardening pass | Must | P5-03 | TODO |  |  |
| P5-09 | Production compose + Caddy | Must | P1-09, D-01, D-02 | TODO |  |  |
| P5-10 | Backups + restore test | Must | P5-09 | TODO |  |  |
| P5-11 | Monitoring profile + alerts | Should | P5-09 | TODO |  |  |
| P5-12 | Load & soak validation | Must | P5-09, P3-09 | TODO |  |  |
| P5-13 | Runbooks + DR drill | Must | P5-10 | TODO |  |  |
| P5-14 | Privacy notice, terms, store disclosures | Must | P5-06 | TODO |  |  |
| P5-15 | Release workflow + deploy task | Must | P5-09, P1-10 | TODO |  |  |
| P5-16 | Final security review + MVP sign-off | Must | every other P5 task | TODO |  |  |

Phase gate: OPEN


### Phase 6

| ID | Title | Pri | Deps | Status | Updated | Notes |
|----|-------|-----|------|--------|---------|-------|
| P6-01 | Usage & identity review | Must | Phase 5 gate | TODO |  |  |
| P6-02 | `PgNotifyBus` + multi-instance | Could | Phase 5 gate | TODO |  |  |
| P6-03 | Zero-downtime deploys | Could | Phase 5 gate | TODO |  |  |
| P6-04 | Read cache | Could | Phase 5 gate | TODO |  |  |
| P6-05 | Slow mode for hot discussions | Should | Phase 5 gate | TODO |  |  |
| P6-06 | DB scaling analysis | Could | Phase 5 gate | TODO |  |  |
| P6-07 | Admin discussion merge | Should | Phase 5 gate | TODO |  |  |
| P6-08 | Search | Could | Phase 5 gate | TODO |  |  |
| P6-09 | Reactions | Could | Phase 5 gate | TODO |  |  |
| P6-10 | Page badges (optional host permission) | Could | Phase 5 gate | TODO |  |  |
| P6-11 | Content adapters for feeds | Could | Phase 5 gate | TODO |  |  |
| P6-12 | Side panel surface | Could | Phase 5 gate | TODO |  |  |
| P6-13 | Abuse automation | Should | Phase 5 gate | TODO |  |  |

Phase gate: OPEN


### Phase 7

| ID | Title | Pri | Deps | Status | Updated | Notes |
|----|-------|-----|------|--------|---------|-------|
| P7-01 | Desktop requirements | Must | Phase 6 gate | TODO |  |  |
| P7-02 | Confirm Tauri 2 | Must | Phase 6 gate | TODO |  |  |
| P7-03 | Shared UI package extraction | Must | Phase 6 gate | TODO |  |  |
| P7-04 | Desktop auth + discussions | Must | Phase 6 gate | TODO |  |  |
| P7-05 | Packaging & updates | Must | Phase 6 gate | TODO |  |  |
| P7-06 | Cross-platform test matrix + release docs | Must | Phase 6 gate | TODO |  |  |

Phase gate: OPEN
