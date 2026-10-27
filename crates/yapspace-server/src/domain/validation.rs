//! Input validation and normalization (docs/architecture/api.md §F.3). The server is authoritative. The extension
//! mirrors these rules only for immediate feedback.

use unicode_normalization::UnicodeNormalization;

use crate::error::FieldError;

pub const RESERVED_USERNAMES: &[&str] = &[
    "admin",
    "administrator",
    "mod",
    "moderator",
    "yapspace",
    "support",
    "help",
    "system",
    "root",
    "api",
    "staff",
    "official",
    "null",
    "undefined",
    "anonymous",
];

pub const MIN_PASSWORD_LEN: usize = 10;
pub const MAX_PASSWORD_LEN: usize = 128;
pub const MAX_BODY_CHARS: usize = 2000;
pub const MAX_TITLE_CHARS: usize = 300;
pub const MAX_LABEL_CHARS: usize = 100;

/// Lowercases and checks a username. Returns the normalized name.
pub fn normalize_username(input: &str) -> Result<String, FieldError> {
    let name = input.trim().to_ascii_lowercase();
    let valid_chars = name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if !(3..=24).contains(&name.len()) || !valid_chars {
        return Err(FieldError::new(
            "username",
            "format",
            "must be 3-24 characters: a-z, 0-9 or _",
        ));
    }
    if RESERVED_USERNAMES.contains(&name.as_str()) || name.starts_with("deleted_") {
        return Err(FieldError::new(
            "username",
            "reserved",
            "this username is not available",
        ));
    }
    Ok(name)
}

/// Checks a password. It must not equal the username, which is compared after lowercasing.
pub fn validate_password(password: &str, username: &str) -> Result<(), FieldError> {
    let len = password.chars().count();
    if !(MIN_PASSWORD_LEN..=MAX_PASSWORD_LEN).contains(&len) {
        return Err(FieldError::new("password", "length", "must be 10-128 characters"));
    }
    if password.to_ascii_lowercase() == username.to_ascii_lowercase() {
        return Err(FieldError::new(
            "password",
            "matches_username",
            "must not equal the username",
        ));
    }
    Ok(())
}

/// NFC-normalizes and trims a message body, then checks it. Line breaks are kept, and runs of three or more are
/// collapsed to a blank line.
pub fn normalize_body(input: &str) -> Result<String, FieldError> {
    let nfc: String = input.nfc().collect();
    let unix = nfc.replace("\r\n", "\n").replace('\r', "\n");
    if unix
        .chars()
        .any(|c| (c.is_control() && c != '\n' && c != '\t') || c == '\u{7f}')
    {
        return Err(FieldError::new(
            "body",
            "control_character",
            "must not contain control characters",
        ));
    }
    let trimmed = unix.trim();
    let collapsed = collapse_blank_lines(trimmed);
    let len = collapsed.chars().count();
    if len == 0 || len > MAX_BODY_CHARS {
        return Err(FieldError::new("body", "length", "must be 1-2000 characters"));
    }
    Ok(collapsed)
}

fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut newlines = 0;
    for c in text.chars() {
        if c == '\n' {
            newlines += 1;
            if newlines <= 2 {
                out.push('\n');
            }
        } else {
            newlines = 0;
            out.push(c);
        }
    }
    out
}

/// Strips control characters, collapses whitespace, and truncates to 300 characters. Returns `None` if nothing is left.
pub fn sanitize_title(input: &str) -> Option<String> {
    let cleaned: String = input
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let truncated: String = collapsed.chars().take(MAX_TITLE_CHARS).collect();
    (!truncated.is_empty()).then_some(truncated)
}

/// Truncates a client label to 100 characters, after removing control characters.
pub fn truncate_label(input: &str) -> Option<String> {
    let cleaned: String = input.chars().filter(|c| !c.is_control()).collect();
    let trimmed = cleaned.trim();
    (!trimmed.is_empty()).then(|| trimmed.chars().take(MAX_LABEL_CHARS).collect())
}

/// `truncate_label` for an optional value.
pub fn truncate_label_opt(input: Option<&str>) -> Option<String> {
    input.and_then(truncate_label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_is_lowercased_and_checked() {
        assert_eq!(normalize_username("Alice_01").unwrap(), "alice_01");
        assert_eq!(normalize_username("ab").unwrap_err().field, "username");
        assert!(normalize_username("Bad-Name").is_err());
        assert!(normalize_username(&"a".repeat(25)).is_err());
        assert_eq!(normalize_username("admin").unwrap_err().code, "reserved");
        assert_eq!(normalize_username("deleted_abc").unwrap_err().code, "reserved");
    }

    #[test]
    fn password_length_and_username_rules() {
        assert!(validate_password("correct horse", "alice").is_ok());
        assert_eq!(validate_password("short", "alice").unwrap_err().code, "length");
        assert_eq!(
            validate_password(&"x".repeat(129), "alice").unwrap_err().code,
            "length"
        );
        assert_eq!(
            validate_password("Alice12345", "alice12345").unwrap_err().code,
            "matches_username"
        );
    }

    #[test]
    fn body_limits_count_characters_not_bytes() {
        assert!(normalize_body(&"é".repeat(2000)).is_ok());
        assert!(normalize_body(&"é".repeat(2001)).is_err());
        // Emoji are multi-byte, so this checks the character count.
        assert!(normalize_body(&"😀".repeat(2000)).is_ok());
        assert!(normalize_body(&"😀".repeat(2001)).is_err());
    }

    #[test]
    fn body_rejects_control_characters_but_keeps_newlines_and_tabs() {
        assert_eq!(
            normalize_body("a\u{0007}b").unwrap_err().code,
            "control_character"
        );
        assert_eq!(normalize_body("a\tb\nc").unwrap(), "a\tb\nc");
        assert_eq!(normalize_body("a\u{7f}").unwrap_err().code, "control_character");
    }

    #[test]
    fn body_normalizes_crlf_collapses_blank_lines_and_trims() {
        assert_eq!(
            normalize_body("  a\r\nb\r\n\r\n\r\n\r\nc  ").unwrap(),
            "a\nb\n\nc"
        );
    }

    #[test]
    fn whitespace_only_body_is_rejected() {
        assert_eq!(normalize_body(" \n\t ").unwrap_err().code, "length");
    }

    #[test]
    fn body_is_nfc_normalized() {
        // "e" followed by a combining acute accent becomes one precomposed "é".
        assert_eq!(normalize_body("e\u{301}").unwrap(), "é");
    }

    #[test]
    fn title_is_collapsed_and_truncated() {
        assert_eq!(
            sanitize_title("  Hello\u{0}   world \n").as_deref(),
            Some("Hello world")
        );
        assert_eq!(sanitize_title("   ").as_deref(), None);
        assert_eq!(sanitize_title(&"x".repeat(400)).unwrap().chars().count(), 300);
    }

    #[test]
    fn label_is_truncated() {
        assert_eq!(truncate_label(&"c".repeat(150)).unwrap().chars().count(), 100);
        assert_eq!(truncate_label("  Chrome\u{1b}  ").as_deref(), Some("Chrome"));
    }
}
