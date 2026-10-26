# URL identity specification

> Source of truth for this topic. Section letters (§X) refer to the original plan; the map is in [docs/plan/README.md](../plan/README.md#section-map). Change decisions only via an ADR / `docs/plan/decisions.md`.

## E. URL identity specification (ruleset v1)

### E.1 Terms and who is authoritative
| # | Term | Produced by | Trusted? |
|---|---|---|---|
| 1 | **Observed URL**: `location.href` | content script | No. It is pre-filtered client-side, then sent once and **never stored** |
| 2 | **Normalized URL**: the generic rules applied to the observed URL | server (`yapspace-identity`) | Deterministic |
| 3 | **Canonical candidate**: `<link rel=canonical>` (or `og:url` if there is no canonical link) | content script → server | No. Accepted only under rule E.5 |
| 4 | **Adapter ID**: a stable content ID from a site adapter (e.g. YouTube video ID) | server adapters | Deterministic, highest priority |
| 5 | **Identity key**: the string that is hashed into `discussion_identities.key_hash` | server | **Authoritative** |

**The server is the only authority on identity.** The client only does privacy pre-filtering (E.7), so it never duplicates the normalization logic.
The `yapspace-identity` crate is pure and versioned. `spec/url-identity/vectors.json` holds the shared conformance tests.

### E.2 Key format
`v1|<kind>|<value>`
- Generic URL: `v1|url|<host><path>[?<sorted query>][#<route fragment>]`, e.g. `v1|url|example.com/news/a?id=3`
- Canonical: `v1|canonical|…`, with the same value format as url (the kind is kept for diagnostics)
- Adapter: `v1|<adapter>|<type>:<id>`, e.g. `v1|youtube|video:dQw4w9WgXcQ`, `v1|x|post:1790000000000000000`,
  `v1|reddit|post:1abcde`, `v1|hn|item:41234567`
- The key is at most 2100 chars. The input URL limit is 2048 chars, otherwise the request is rejected with `url_unsupported/too_long`.
- `key_hash = sha256(key bytes)`.

### E.3 Generic normalization, step by step (must be implemented in exactly this order)
1. **Parse** with the `url` crate (WHATWG). If it fails → `invalid`. If the input is longer than 2048 bytes → `too_long`.
2. **Scheme** must be `http` or `https`, otherwise → `unsupported_scheme`. The scheme is **not** part of the key (http and https share a discussion).
3. **Userinfo**: if present, drop it (never stored).
4. **Host**: the `url` crate already lowercases and IDNA-encodes it to punycode ASCII (`xn--…`). Strip a single trailing dot. Then reject:
   IP literals (v4/v6) → `ip_host`; `localhost`, any host ending in `.localhost`, `.local`, `.internal`, `.lan`, `.home.arpa`, or any
   single-label host (no dot) → `private_host`; hosts in `BLOCKED_HOSTS` (E.6) → `blocked_host`.
   Then strip **one** leading `www.` label (only if what's left still contains a dot).
5. **Port**: the `url` crate drops default ports. A non-default port stays in the key as `host:port`.
6. **Path**:
   a. If it contains `;jsessionid=` (case-insensitive) or any `;` path parameter → reject `sensitive_parameters`.
   b. Percent-encoding: decode the octets of unreserved chars (`A–Z a–z 0–9 - . _ ~`) and uppercase the hex of the rest.
   c. Dot segments are already removed by WHATWG parsing.
   d. Collapse repeated slashes `//` → `/`.
   e. Remove one trailing `/` unless the path is just `/`. Path case is **preserved**.
   f. Secret check: if any path segment matches `SECRET_LIKE` (E.6) → reject `secret_like_token`.
7. **Query**:
   a. Split into `(key, value)` pairs and keep empty values. Percent-normalize both as in 6b. `+` stays literal (no space conversion).
   b. If any key (lowercased) is in `SENSITIVE_PARAMS` or starts with a `SENSITIVE_PREFIXES` entry → reject `sensitive_parameters`.
   c. If any value matches `SECRET_LIKE` → reject `secret_like_token`.
   d. Drop pairs whose lowercased key is in `TRACKING_PARAMS` or starts with `utm_`.
   e. Sort the remaining pairs by key, then by value (byte order). Duplicate pairs are kept, because some sites use repeated keys.
   f. If nothing is left, there is no `?`.
8. **Fragment**: drop it, **unless** it starts with `/` or `!/` (hash routing, e.g. `#/inbox/123`). In that case apply step 6 to
   the fragment's path part and drop any query inside the fragment. Fragments like `#access_token=…` are always dropped.
9. Build `value = host[:port] + path + ["?" + query] + ["#" + routefrag]`. The `display_url = "https://" + value` (or `http://` if
   the observed URL was http).

### E.4 Site adapters (v1), applied before the generic rules when the host matches
| Adapter | Hosts | Patterns → key | display_url |
|---|---|---|---|
| youtube | youtube.com, m.youtube.com, music.youtube.com, youtu.be, youtube-nocookie.com | `/watch?v=ID`, `/shorts/ID`, `/live/ID`, `/embed/ID`, `youtu.be/ID` where ID matches `^[A-Za-z0-9_-]{11}$` → `youtube|video:ID`. Ignore `t`, `list`, `index`, `si`, `feature`, `pp` | `https://www.youtube.com/watch?v=ID` |
| x | x.com, twitter.com, mobile.x.com, mobile.twitter.com | `/{user}/status/{digits}` (with optional trailing `/photo/N`, `/video/N`, `/analytics`) and `/i/web/status/{digits}` → `x|post:DIGITS` | `https://x.com/i/status/DIGITS` |
| reddit | reddit.com, old/new/np/m.reddit.com, redd.it | `/r/{sub}/comments/{id}/…`, `/comments/{id}`, `redd.it/{id}` where id matches `^[a-z0-9]{3,10}$` (lowercased) → `reddit|post:id` (comment permalinks map to the post in v1) | `https://www.reddit.com/comments/id` |
| hn | news.ycombinator.com | `/item?id={digits}` → `hn|item:DIGITS` | `https://news.ycombinator.com/item?id=DIGITS` |

The host check for adapters happens after step 4 (lowercase, `www.` stripped). If an adapter matches the host but **not** a pattern (e.g. a YouTube
channel page), fall back to the generic rules. Adapters live in `yapspace-identity/src/adapters/{youtube,x,reddit,hn}.rs` behind a
`trait SiteAdapter { fn id(&self) -> &'static str; fn matches_host(&self, host: &str) -> bool; fn key(&self, url: &Url) -> Option<AdapterKey>; }`.
Adding an adapter changes the keys for that site, so **it requires a ruleset bump** (E.8), or it must only apply to URLs that had no discussion before (always do the
legacy lookup).

### E.5 Choosing the identity (resolution algorithm)
Input: `observed` (required), `canonical` (optional). Output: an ordered list of candidate keys plus the primary key used for creation.
1. `obs = normalize(observed)`. If it fails → return the error (the panel shows "YapSpace isn't available on this page").
2. If the adapter matched `observed` → candidates = `[adapter_key]`. Done (the canonical is ignored).
3. If `canonical` is present, try `can = normalize(canonical)` and accept it only if **all** of these hold:
   - it normalizes OK, and the adapter rule doesn't apply (if the canonical matches an adapter, use that adapter key, also kind `adapter`);
   - its registrable domain (eTLD+1, via the `psl` crate) equals the observed registrable domain;
   - NOT (`obs` path != `/` AND `can` path == `/`), because a deep page pointing its canonical at the homepage is a known CMS bug;
   - the canonical value differs from the observed value (otherwise it adds nothing).
4. candidates = `[can_key (kind canonical)] + [obs_key (kind url)]` when the canonical was accepted, otherwise `[obs_key]`.
5. **Lookup:** run Q1 for each candidate in order. The first hit wins.
6. **Create** (only through `POST /v1/discussions`): insert using `candidates[0]`. **No implicit aliases**: we never store more than the
   primary key on creation. This is deterministic and prevents a bad canonical from merging pages for good.
7. **SPA guard (client side):** the content script sends `canonical` only on the first context after injection, or after a
   `locationchange` if the canonical element's `href` **changed** since the previous context. A stale canonical (SPA navigated, tag not
   updated) is therefore not sent.

### E.6 Lists (constants in `yapspace-identity/src/rules_v1.rs`; any change = ruleset bump unless it only *adds* rejections)
- `TRACKING_PARAMS` (exact, lowercase): `fbclid gclid gclsrc dclid gbraid wbraid msclkid yclid twclid ttclid li_fat_id mc_cid mc_eid
  _ga _gl _hsenc _hsmi __hssc __hstc __hsfp hsctatracking igshid igsh mkt_tok oly_anon_id oly_enc_id vero_id wickedid rb_clickid
  s_cid sc_cid spm scm ref_src ref_url cmpid ncid ocid sr_share at_medium at_campaign` plus the prefix `utm_`.
- `SENSITIVE_PARAMS` (exact, lowercase → reject): `access_token id_token refresh_token token auth auth_token authorization code
  password passwd pwd pass secret client_secret api_key apikey session sessionid session_id jsessionid phpsessid sid sig signature
  otp email mail reset_token invite invite_code ticket magic_link login_token`.
- `SENSITIVE_PREFIXES`: `x-amz- x-goog- oauth_`.
- `SECRET_LIKE`: a value or segment ≥ 32 chars of `[A-Za-z0-9_-]` that contains **at least one uppercase letter, one lowercase letter and one digit**.
  This catches base64url tokens and keeps lowercase-hex commit SHAs and slugs.
- `BLOCKED_HOSTS` (exact or as a suffix with a leading dot): `mail.google.com docs.google.com drive.google.com meet.google.com
  calendar.google.com accounts.google.com outlook.live.com outlook.office.com outlook.office365.com teams.microsoft.com
  onedrive.live.com 1drv.ms dropbox.com dl.dropboxusercontent.com zoom.us web.whatsapp.com web.telegram.org messenger.com
  discord.com app.slack.com notion.so figma.com chrome.google.com chromewebstore.google.com addons.mozilla.org`. These are private apps or
  **capability URLs**, where showing the URL publicly would leak access. Admins can extend the list via config `YAPSPACE_EXTRA_BLOCKED_HOSTS`.

### E.7 Client-side pre-filter (extension, `src/lib/page-url.ts`). It is deliberately minimal
Before the extension sends anything: only `http:`/`https:` pages; remove the fragment unless it starts with `/` or `!/`; remove userinfo; do not send if the
host is an IP literal or `localhost`. Nothing more. The server does everything else and **never persists the observed URL or the
canonical URL**. It stores only the normalized key and display_url of an accepted identity, and only when a user explicitly creates a
discussion. `resolve` is a POST so that URLs never appear in proxy access logs, and Caddy access logging is off.

### E.8 Versioning, legacy and merges
- The ruleset version is part of every key (`v1|`). `discussion_identities.ruleset` records it.
- When a ruleset changes (v2): the resolver computes the v2 candidates **and then** the v1 candidates, and looks them up in that order (legacy lookup).
  On a v1 hit it lazily inserts the v2 key as kind `legacy` → the same discussion (`ON CONFLICT DO NOTHING`). New discussions use v2 keys.
  **Existing discussions are never merged automatically.**
- **Merges are admin-only** (Phase 6, P6-07): set `merged_into_id` on the source, re-point the source's identity rows (`kind='merge'`)
  to the target, re-point earlier merges into the source, and record a `moderation_actions` row. Messages stay in the source discussion, which
  can still be read via the "merged" banner. Users report wrong associations with report reason `wrong_discussion`.

### E.9 Test vectors (`spec/url-identity/vectors.json`, at least 80 cases, written in P0-04)
```json
[
  {"name": "drops utm and www, sorts query", "observed": "https://www.Example.com/a/b/?utm_source=x&z=1&a=2#top",
   "canonical": null, "expect": {"key": "v1|url|example.com/a/b?a=2&z=1", "display_url": "https://example.com/a/b?a=2&z=1"}},
  {"name": "http and https same", "observed": "http://example.com/x", "expect": {"key": "v1|url|example.com/x"}},
  {"name": "idn punycode", "observed": "https://bücher.de/", "expect": {"key": "v1|url|xn--bcher-kva.de/"}},
  {"name": "non-default port kept", "observed": "https://example.com:8443/x", "expect": {"key": "v1|url|example.com:8443/x"}},
  {"name": "hash route kept", "observed": "https://app.example.com/#/board/42?tab=1", "expect": {"key": "v1|url|app.example.com/#/board/42"}},
  {"name": "token fragment dropped", "observed": "https://example.com/cb#access_token=abc", "expect": {"key": "v1|url|example.com/cb"}},
  {"name": "sensitive param rejected", "observed": "https://example.com/reset?token=abc", "expect": {"error": "sensitive_parameters"}},
  {"name": "secret-like segment", "observed": "https://example.com/share/aB3dEfGhIjKlMnOpQrStUvWxYz012345", "expect": {"error": "secret_like_token"}},
  {"name": "commit sha allowed", "observed": "https://github.com/o/r/commit/0123456789abcdef0123456789abcdef01234567", "expect": {"key": "v1|url|github.com/o/r/commit/0123456789abcdef0123456789abcdef01234567"}},
  {"name": "youtube short link", "observed": "https://youtu.be/dQw4w9WgXcQ?si=abc&t=10", "expect": {"key": "v1|youtube|video:dQw4w9WgXcQ", "display_url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ"}},
  {"name": "x status", "observed": "https://twitter.com/jack/status/20/photo/1", "expect": {"key": "v1|x|post:20"}},
  {"name": "reddit comment maps to post", "observed": "https://old.reddit.com/r/rust/comments/1abcde/title/kxyz12/", "expect": {"key": "v1|reddit|post:1abcde"}},
  {"name": "hn item", "observed": "https://news.ycombinator.com/item?id=41234567&p=2", "expect": {"key": "v1|hn|item:41234567"}},
  {"name": "canonical accepted", "observed": "https://news.example.com/a?ref=home", "canonical": "https://news.example.com/a",
   "expect": {"candidates": ["v1|canonical|news.example.com/a", "v1|url|news.example.com/a?ref=home"]}},
  {"name": "canonical cross-site rejected", "observed": "https://blog.a.com/p", "canonical": "https://evil.com/p", "expect": {"candidates": ["v1|url|blog.a.com/p"]}},
  {"name": "canonical to homepage rejected", "observed": "https://a.com/deep/page", "canonical": "https://a.com/", "expect": {"candidates": ["v1|url|a.com/deep/page"]}},
  {"name": "ip host rejected", "observed": "http://192.168.1.1/admin", "expect": {"error": "ip_host"}},
  {"name": "blocked capability host", "observed": "https://docs.google.com/document/d/1x/edit", "expect": {"error": "blocked_host"}},
  {"name": "file scheme", "observed": "file:///C:/x.html", "expect": {"error": "unsupported_scheme"}}
]
```
`expect` fields are optional: assert only the fields that are present. `key` = candidates[0]. The Rust test `crates/yapspace-identity/tests/vectors.rs`
loads this file via `include_str!("../../../spec/url-identity/vectors.json")`. P0-04 extends the list to at least 80 cases covering every rule in E.3–E.6
(trailing slash, `//`, percent-encoding case, empty query, repeated keys, `+`, `www.` only stripped once, `www.co` edge, `.local`, uppercase
scheme, very long URL, `;jsessionid`, `x-amz-` prefix, each adapter's non-matching paths, m.youtube, shorts, embed, music, twitter mobile, i/web/status,
redd.it, reddit `/r/sub/s/token` share link → generic, etc.).
