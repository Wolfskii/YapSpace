# Risk register

> Source of truth for this topic. Section letters (§X) refer to the original plan; the map is in [docs/plan/README.md](../plan/README.md#section-map). Change decisions only via an ADR / `docs/plan/decisions.md`.

## P. Risk register
| ID | Risk | Prob. | Impact | Mitigation | Validation |
|---|---|---|---|---|---|
| R-01 | URL identity splits a discussion that should be one, or merges discussions that should be separate | High | High | deterministic rules, adapters, no implicit aliases, `wrong_discussion` reports, admin merge | vectors (P0-04), report-rate metric (P6-01) |
| R-02 | Spam/abuse overwhelms a small moderation capacity | High | High | rate limits, new-account limits, dup detection, reports, suspensions, a kill switch on registration | abuse drills in P5-12, report metrics |
| R-03 | The extension iframe is blocked or broken on some sites (CSP/frame rules, odd CSS) | Med | Med | closed shadow host, inline styles, the fallback design in §I.1 | P4-02 spike on 20 top sites |
| R-04 | Store review rejects the extension or flags permissions | Med | High | minimal permissions (activeTab), clear justifications, a privacy disclosure | P4-12 checklist, early submission of an unlisted build |
| R-05 | Privacy leak from sensitive URLs | Med | High | refusal rules, POST resolve, no raw storage, blocked hosts | vectors + security review P5-16 |
| R-06 | A viral discussion overloads the single box | Low (early) | High | slow mode, broadcast lag handling, scaling triggers | the chat-viral load test |
| R-07 | Data loss | Low | Critical | daily encrypted off-site backups, weekly restore test | P5-10 restore test |
| R-08 | Legal/regulatory obligations (GDPR, DSA notice-and-action) | Med | High | minimization, export/delete, report flow, a legal review | D-05/D-06 before launch |
| R-09 | The cheaper implementation model drifts from the specs | Med | Med | detailed specs, vectors, `task check` gate, NEEDS_DECISION protocol, periodic human review | review at each phase gate |
| R-10 | Crate API churn (sqlx 0.9, utoipa 6, argon2 0.6 are recent majors) | Med | Low | versions pinned, the docs.rs-wins rule | compile in P1 |
| R-11 | Single-VPS outage | Med | Med | a documented 2 h rebuild runbook, uptime alerting | DR drill once before launch (P5-13) |
| R-12 | Username/password without email → account recovery support burden | Med | Low | the recovery code, clear UI warning | support metrics after launch |
