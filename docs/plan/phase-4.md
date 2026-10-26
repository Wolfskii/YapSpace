# Phase 4 · Browser extension MVP

> Task specs for this phase. **Status lives only in the board:** [README.md](README.md). Architecture refs (§X) → see the [section map](README.md#section-map).

**Prerequisites:** P3-01 DONE (protocol), Phase 2 PASSED (the API is stable). P4-09 needs P3-05.
**Exit criteria:** on Chrome and Firefox (manually checked against the checklist), a user can open the panel on a normal article, register, post, reply, see live
messages from a second browser, recover after the network drops, and see clear states on unsupported pages. CI builds both zips. The Playwright e2e is green.

### P4-01 · Scaffold WXT + Svelte
**Priority:** Must · **Deps:** P1-10 · **Spec:** §I.1–I.2, §N.2
**Steps:**
- [ ] 1. From the repo root: `npx wxt@0.21 init extension --template svelte --pm npm` (if the flags differ, run it interactively and choose svelte + npm).
- [ ] 2. Make sure `package.json` has `"name": "yapspace-extension"`, `"private": true`, and the scripts `dev`, `dev:firefox` (`wxt -b firefox --mv3`),
  `build`, `build:firefox` (`wxt build -b firefox --mv3`), `zip`, `zip:firefox`, `check` (`svelte-check --tsconfig ./tsconfig.json`),
  `lint` (`eslint . && prettier --check .`), `format`, `test` (`vitest run`), `e2e` (`playwright test`), `types:api` (openapi-typescript).
- [ ] 3. Remove the template demo code. Add the folders from §I.2 with placeholder files.
- [ ] 4. `wxt.config.ts`:
```ts
import { defineConfig } from 'wxt';
export default defineConfig({
  modules: ['@wxt-dev/module-svelte'],
  manifest: ({ browser, mode }) => ({
    name: 'YapSpace',
    description: 'The internet is better when we talk.',
    permissions: ['activeTab', 'scripting', 'storage'],
    host_permissions: [new URL(import.meta.env.WXT_API_BASE_URL ?? 'http://localhost:8080').origin + '/*'],
    action: { default_title: 'Open YapSpace' },
    commands: { 'toggle-panel': { suggested_key: { default: 'Alt+Y' }, description: 'Toggle YapSpace' } },
    web_accessible_resources: [{ resources: ['panel.html'], matches: ['<all_urls>'],
                                 ...(browser === 'chrome' ? { use_dynamic_url: true } : {}) }],
    ...(browser === 'firefox' ? { browser_specific_settings: { gecko: { id: 'extension@yapspace.app', strict_min_version: '128.0' } } } : {}),
    default_locale: 'en',
  }),
});
```
  (If `import.meta.env` isn't available in the config, read the value from `process.env` via `loadEnv` per the WXT docs. Record what works in HANDOFF.)
- [ ] 5. eslint flat config with `eslint-plugin-svelte` + `typescript-eslint` + the rule `svelte/no-at-html-tags: error`. Prettier with the svelte plugin.
- [ ] 6. Add the `ext:*` tasks to the Taskfile and add the `extension` job to `ci.yml` (§M.3) + `web-ext lint`.
**Acceptance:** `task ext:build` produces `.output/chrome-mv3` and `.output/firefox-mv3`. Loading them unpacked shows the YapSpace action. `task check` is green.

### P4-02 · Spike: injection model on real sites (validates ADR 0011)
**Priority:** Must · **Deps:** P4-01 · **Spec:** §I.3 steps 1–2, Risk R-03
**Deliverables:** `background.ts` action/command → `executeScript` + the failure badge. `panel-host.content.ts` (`registration: 'runtime'`) creates the host +
iframe. `panel/` shows a static "Hello from YapSpace" page that reports `location.href` of the parent via the background (no API calls yet).
`docs/extension/spike-results.md`: test on Chrome **and** Firefox on ≥ 20 sites (youtube.com watch, x.com status, reddit.com post, news.ycombinator.com item,
github.com repo, wikipedia article, nytimes/bbc article, medium post, stackoverflow question, amazon product, a Google search results page, gmail (should
still inject, but the server will later refuse it), a site with strict CSP (github.com), an SPA (x.com), the Chrome Web Store (must fail gracefully),
a PDF, `about:blank`, a `file://` page). Record per site: injects? iframe visible? styles intact? toggles?
**Acceptance:** works on ≥ 18/20 regular sites in both browsers. If the iframe is blocked anywhere important → set `NEEDS_DECISION` with a proposal (the shadow-root
fallback). Update ADR 0011 to Accepted if it passes.
**Notes:** the WXT output path of runtime content scripts is usually `/content-scripts/panel-host.js`. Check `.output/chrome-mv3/` and use that exact path.

### P4-03 · Page context pipeline
**Priority:** Must · **Deps:** P4-02 · **Spec:** §I.3 steps 2–3, §E.5 step 7, §E.7, §H.3
**Deliverables:** `src/lib/messages.ts` (types + guards: `PageContextMsg {type:'page-context', url, canonical: string|null, title}`,
`PanelCommand {type:'panel-close'|'panel-minimize'}`, `PortMsg {type:'context', context} | {type:'close'}`), `src/lib/page-url.ts` (the pre-filter),
the content-script canonical reading (`document.querySelector('link[rel="canonical" i]')?.href ?? meta[property="og:url"]`) with the SPA guard, and the background registry.
The panel shows the context it receives (debug view).
**Tests:** vitest for `page-url.ts` (≥ 12 cases), the guards, and the background routing with `fakeBrowser` (a context for tab 1 doesn't leak to the port of tab 2).
**Acceptance:** navigating inside an SPA updates the panel's context within 500 ms.

### P4-04 · API client + generated types
**Priority:** Must · **Deps:** P4-01, P2-21
**Deliverables:** `task ext:types` → `src/lib/api/schema.d.ts`. `src/lib/api/client.ts`: `createClient<paths>({ baseUrl })` from `openapi-fetch` + middleware that
adds `Authorization` from the session store, and on a 401 for an authed request clears the session. `ApiError { status, code, detail, fields, retryAfterS, reason }`
parsed from problem+json. A network failure → `ApiError{code:'network'}`. Config: `import.meta.env.WXT_API_BASE_URL` and `WXT_WS_URL`, with `extension/.env.example`.
**Tests:** vitest with a mocked fetch: problem+json parsing, the auth header added only for the configured origin, the 401 behavior.

### P4-05 · Session store + auth UI
**Priority:** Must · **Deps:** P4-04 · **Spec:** §H.1, §I.4
**Deliverables:** `state/session.svelte.ts` (`$state` user/token. Load from `storage.local['ys.session']` at startup. `register/login/logout/recover`. Sync between open panels with
`storage.onChanged`). `AuthForm.svelte` (tabs: Sign in / Create account / Recover. Field validation that mirrors the server rules for UX only, while the server stays
authoritative. Server field errors are shown per field). `RecoveryCodeNotice.svelte` (shown after register/recover: the code in monospace, a copy button,
"I've saved it" checkbox required to continue, plus a warning that without it a lost password can't be recovered).
**Tests:** component tests for validation messages, the recovery notice gating, and the logout clearing storage.

### P4-06 · Discussion store (state machine + pagination)
**Priority:** Must · **Deps:** P4-05, P4-03 · **Spec:** §I.3 step 4, §F.1
**Deliverables:** `state/discussion.svelte.ts`: states `idle | resolving | unsupported{reason} | none{identity} | loading | ready | error{retry}`.
`messages: Map<number /*seq*/, Message>` + a derived sorted array. `loadNewest()`, `loadOlder()` (uses next_before_seq, guards against concurrent loads),
`applyServerMessage(m)` (upsert by seq), `pending: Map<client_msg_id, PendingMessage>`. Handles a context change (abort in-flight requests with `AbortController`).
**Tests:** state transitions for each resolve outcome. loadOlder merges without duplicates. A context change mid-load discards the stale results.

### P4-07 · Message list UI
**Priority:** Must · **Deps:** P4-06 · **Spec:** §I.4, §H.2 (XSS)
**Deliverables:** `MessageList.svelte` (`role="log"`, scroll anchoring when older messages are prepended, a top sentinel IntersectionObserver → loadOlder, a "New messages" pill,
a 1000-item DOM cap), `MessageItem.svelte` (author, relative time with an absolute time in `title`, the parent preview chip (click → scroll to the parent if loaded),
the body rendered via `linkify()` tokens as text and `<a>` (http/https only, `target="_blank" rel="noopener noreferrer nofollow ugc"`),
tombstones "message deleted"/"removed by moderators", and an actions menu: reply, report, delete (own)). `src/lib/linkify.ts`.
**Tests:** linkify (javascript:, data:, `http://` inside text, a trailing punctuation strip), MessageItem renders `<img src=x onerror=alert(1)>` as literal text,
axe passes.

### P4-08 · Composer (post, reply, first post creates the discussion)
**Priority:** Must · **Deps:** P4-07 · **Spec:** §I.3 step 5
**Deliverables:** `Composer.svelte` (an autosizing textarea, a counter turning red > 1900 chars, Enter sends and Shift+Enter adds a newline, a reply chip with cancel,
disabled with "Sign in to join" when signed out, a cooldown countdown on 429, a notice for locked/suspended). The send flow: generate `client_msg_id` → pending item →
ensure the discussion exists (`POST /v1/discussions` in state `none`) → `POST …/messages` → replace the pending item → on a network error, retry ×3 with backoff and the
same id → a final failure marks the pending item "failed – retry".
**Tests:** a component test for the send flow with a mocked client: idempotent retry uses the same client_msg_id, the first post creates the discussion and then the message.

### P4-09 · Realtime client integration
**Priority:** Must · **Deps:** P4-08, P3-05 · **Spec:** §G.4
**Deliverables:** `src/lib/realtime.ts` `class RealtimeClient { connect(); subscribe(id, afterSeq); unsubscribe(id); onEvent(cb); onStatus(cb) }` with the
backoff/jitter, app ping/pong timeout, online/visibility handlers, and resubscribe-on-reconnect using each discussion's current `lastSeq` (read through a callback from the store).
Wire it into the discussion store (subscribe after loadNewest using its max seq; `message.created` → applyServerMessage; `resync_required` → loadNewest + resubscribe).
`OfflineBanner.svelte` shows "Reconnecting in Ns…" and "Live updates unavailable".
**Tests:** vitest with a fake WebSocket class: backoff sequence bounds, resubscribe after reconnect carries after_seq, dedupe, resync handling, a dead-socket
detection via a missing pong.

### P4-10 · Report + delete own message (client)
**Priority:** Must · **Deps:** P4-09 · **Note:** the report API only arrives in P5-02. Build the dialog now, but gate the report actions behind a
constant `FEATURES.reports = false` in `src/lib/features.ts`. P5-02 flips it to `true`.
**Deliverables:** `ReportDialog.svelte` (the reason radio list from §D.2, optional details ≤ 1000, submit, success toast). Delete with a confirmation.
"Report wrong discussion" in the header menu (target = the discussion, reason `wrong_discussion`).
**Tests:** dialog focus management (focus trap, Esc closes, focus returns to the trigger).

### P4-11 · States, a11y, i18n polish
**Priority:** Must · **Deps:** P4-09
**Deliverables:** `StateView.svelte` for every state in §I.4 with human copy (all of it in `_locales/en/messages.json`). The minimize bubble. The dark/light theme from
`prefers-color-scheme`. Reduced motion. An axe check over every state in component tests. Keyboard-only walkthrough notes in the manual checklist.
**Acceptance:** no hard-coded user-visible strings (a grep check: `rg -n ">[A-Za-z][^<{]*<" extension/src/components` returns nothing, or only documented exceptions).

### P4-12 · Cross-browser validation + store packaging docs
**Priority:** Must · **Deps:** P4-11
**Deliverables:** `docs/extension/manual-test-checklist.md` (≈ 30 checks: install, toggle via click and shortcut, unsupported pages, register/login/logout,
post/reply/delete/report, live updates between two browsers, airplane-mode recovery, laptop sleep for 2 min recovery, SPA navigation, minimize, theme, screen
reader smoke with NVDA on Chrome), run on Chrome + Firefox with the results recorded. `docs/extension/store-submission.md` (Chrome/Edge/Firefox steps, the
permission justifications, privacy practices answers, the AMO source-code zip instructions `wxt zip -b firefox` + `--sources`).
Placeholder icons in 4 sizes.
**Acceptance:** the checklist passes on both browsers (set `IN_REVIEW` until a human confirms the Firefox run if the agent can't drive Firefox).

### P4-13 · Playwright e2e (Chromium)
**Priority:** Should · **Deps:** P4-11
**Deliverables:** `e2e-autoload.content.ts` (only when `import.meta.env.MODE === 'e2e'`, using WXT's `include`/conditional entrypoint support), `playwright.config.ts`
with a `webServer` for the fixtures (`npx serve e2e/fixtures -l 4173`), and a persistent context with `--disable-extensions-except` + `--load-extension`
pointing at `.output/chrome-mv3` (built with `wxt build --mode e2e`). Fixtures: `article.html` (canonical), `spa.html` (pushState), `hostile.html`
(aggressive CSS + a strict CSP meta tag), `stale-canonical.html`. Specs: `post-and-live.spec.ts` (two contexts), `spa-navigation.spec.ts`,
`hostile-page.spec.ts` (the panel is still visible and usable).
**Acceptance:** `npm run e2e` is green locally with the backend running. Added to `nightly.yml`.
