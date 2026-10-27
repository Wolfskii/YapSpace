//! Rule lists for ruleset v1 (docs/architecture/url-identity.md §E.6). A change to any list that only adds
//! rejections may ship without a ruleset bump. Any other change needs a new ruleset.

/// Tracking parameters removed from queries (exact match, lowercase).
pub const TRACKING_PARAMS: &[&str] = &[
    "fbclid",
    "gclid",
    "gclsrc",
    "dclid",
    "gbraid",
    "wbraid",
    "msclkid",
    "yclid",
    "twclid",
    "ttclid",
    "li_fat_id",
    "mc_cid",
    "mc_eid",
    "_ga",
    "_gl",
    "_hsenc",
    "_hsmi",
    "__hssc",
    "__hstc",
    "__hsfp",
    "hsctatracking",
    "igshid",
    "igsh",
    "mkt_tok",
    "oly_anon_id",
    "oly_enc_id",
    "vero_id",
    "wickedid",
    "rb_clickid",
    "s_cid",
    "sc_cid",
    "spm",
    "scm",
    "ref_src",
    "ref_url",
    "cmpid",
    "ncid",
    "ocid",
    "sr_share",
    "at_medium",
    "at_campaign",
];

/// Tracking parameter prefixes removed from queries.
pub const TRACKING_PREFIXES: &[&str] = &["utm_"];

/// Query parameters whose presence means the URL carries a secret. Such URLs are refused (exact match, lowercase).
pub const SENSITIVE_PARAMS: &[&str] = &[
    "access_token",
    "id_token",
    "refresh_token",
    "token",
    "auth",
    "auth_token",
    "authorization",
    "code",
    "password",
    "passwd",
    "pwd",
    "pass",
    "secret",
    "client_secret",
    "api_key",
    "apikey",
    "session",
    "sessionid",
    "session_id",
    "jsessionid",
    "phpsessid",
    "sid",
    "sig",
    "signature",
    "otp",
    "email",
    "mail",
    "reset_token",
    "invite",
    "invite_code",
    "ticket",
    "magic_link",
    "login_token",
];

/// Parameter name prefixes whose presence means the URL carries a secret.
pub const SENSITIVE_PREFIXES: &[&str] = &["x-amz-", "x-goog-", "oauth_"];

/// Minimum length of a value or path segment that counts as a secret when it mixes upper case, lower case and digits.
pub const SECRET_LIKE_MIN_LEN: usize = 32;

/// Hosts that are private apps or capability URLs. Showing them publicly would leak access. Matched exactly or as a
/// parent domain (`docs.google.com` also matches `x.docs.google.com`).
pub const BLOCKED_HOSTS: &[&str] = &[
    "mail.google.com",
    "docs.google.com",
    "drive.google.com",
    "meet.google.com",
    "calendar.google.com",
    "accounts.google.com",
    "outlook.live.com",
    "outlook.office.com",
    "outlook.office365.com",
    "teams.microsoft.com",
    "onedrive.live.com",
    "1drv.ms",
    "dropbox.com",
    "dl.dropboxusercontent.com",
    "zoom.us",
    "web.whatsapp.com",
    "web.telegram.org",
    "messenger.com",
    "discord.com",
    "app.slack.com",
    "notion.so",
    "figma.com",
    "chrome.google.com",
    "chromewebstore.google.com",
    "addons.mozilla.org",
];

/// Host suffixes that are never public: local names and private DNS zones.
pub const PRIVATE_HOST_SUFFIXES: &[&str] = &[".localhost", ".local", ".internal", ".lan", ".home.arpa"];
