# Phase 0 · Architecture validation

> Task specs for this phase. **Status lives only in the board:** [README.md](README.md). Architecture refs (§X) → see the [section map](README.md#section-map).

**Prerequisites:** none. **Parallelizable:** P0-03 and P0-04 can run alongside Phase 1 (they're marked parallel-OK).
**Exit criteria:** the plan and docs are in the repo, the ADRs exist, there are ≥ 80 vectors, and every D-xx question is logged.

### P0-01 · Save the plan into the repo — *done by the planning session straight after approval*
**Priority:** Must · **Deps:** — · **Spec:** §1
**Deliverables:** every file in the §1.1 map, plus `.gitignore` (Rust `target/`, `node_modules/`, `.output/`, `.wxt/`, `.env`, `.env.*` except
`*.example`, `deploy/secrets/`, `deploy/.env`, `deploy/backup.env`, `*.log`), `.gitattributes`, `.editorconfig`, and a minimal `README.md`
(name, tagline, "start here → AGENTS.md").
**Steps:** - [x] write the files · - [x] board lists every task in §Q with `TODO` (P0-01 `DONE`) · - [x] commit `docs: added yapspace implementation plan`
**Acceptance:** `git log` shows one commit with no trailers. Every link in `docs/plan/README.md` resolves.

### P0-02 · Architecture diagrams check — parallel-OK
**Priority:** Should · **Deps:** P0-01 · **Spec:** §C
**Deliverables:** `docs/architecture/overview.md` gains two more Mermaid diagrams: (1) a sequence diagram for "open panel → resolve → history → subscribe",
(2) a sequence diagram for "post → tx → commit → publish → fan-out → client dedupe".
**Acceptance:** the diagrams render on GitHub (valid Mermaid: check with `npx -y @mermaid-js/mermaid-cli -i file -o out.svg` on an extracted block).

### P0-03 · Write ADRs 0001–0015 — parallel-OK
**Priority:** Must · **Deps:** P0-01 · **Spec:** §O + the referenced sections
**Deliverables:** `docs/adr/README.md` (index table) + 15 files `docs/adr/0001-backend-framework.md` … `0015-desktop-framework.md`, using the §O template.
**Steps:** one checkbox per ADR. Copy the alternatives and reasons from the §A table and the relevant section. Don't invent new decisions.
**Acceptance:** 15 files, each with all 8 template headings. Status matches §O (0007, 0011 and 0015 are Provisional).

### P0-04 · URL identity test vectors — parallel-OK
**Priority:** Must · **Deps:** P0-01 · **Spec:** §E (whole)
**Deliverables:** `spec/url-identity/vectors.json` (≥ 80 cases) + `spec/url-identity/README.md` (format, the rule that a vector is never edited, only
superseded with `"ruleset": 2` and a new name).
**Steps:**
- [ ] 1. Start from the 19 examples in §E.9.
- [ ] 2. Add ≥ 3 cases per rule in E.3 steps 1–8, ≥ 4 per adapter (2 matching, 2 non-matching → generic), ≥ 6 for canonical acceptance/rejection,
  and ≥ 1 per `TRACKING_PARAMS` family and per rejection reason.
- [ ] 3. Validate the JSON (`node -e "JSON.parse(require('fs').readFileSync('spec/url-identity/vectors.json','utf8'))"`) and check that names are unique.
**Acceptance:** ≥ 80 cases, unique names, and every `error` value is one of the §F.1 reasons.
**Notes:** expected keys are worked out by hand from the rules. If a rule seems ambiguous, log it in decisions.md rather than guessing.

### P0-05 · Decisions log
**Priority:** Must · **Deps:** P0-01 · **Deliverables:** `docs/plan/decisions.md` with D-01…D-07 from §B (status `NEEDS_DECISION`, the recommendation,
and what each one blocks). The board shows them under "Open decisions". *Done together with P0-01.*
