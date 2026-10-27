//! Session tokens and recovery codes (docs/architecture/security.md §H.1). Only SHA-256 hashes are stored.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

/// Prefix on every session token, so that tokens are easy to recognize in logs and secret scanners.
pub const TOKEN_PREFIX: &str = "ys_";

/// Crockford base32 alphabet. It has no I, L, O or U, so codes are hard to misread.
const RECOVERY_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const RECOVERY_LEN: usize = 20;

/// Returns a new session token and its hash. The token goes to the client once.
pub fn new_session_token() -> (String, [u8; 32]) {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    let token = format!("{TOKEN_PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes));
    let hash = hash_token(&token);
    (token, hash)
}

pub fn hash_token(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

/// Returns a new recovery code (`XXXXX-XXXXX-XXXXX-XXXXX`) and the hash of its normalized form.
pub fn new_recovery_code() -> (String, [u8; 32]) {
    let mut raw = [0u8; RECOVERY_LEN];
    rand::fill(&mut raw);
    // 256 is a multiple of 32, so taking each byte modulo 32 is unbiased.
    let chars: Vec<u8> = raw
        .iter()
        .map(|b| RECOVERY_ALPHABET[(*b % 32) as usize])
        .collect();
    let code = chars
        .chunks(5)
        .map(|c| std::str::from_utf8(c).expect("ascii alphabet"))
        .collect::<Vec<_>>()
        .join("-");
    let hash = hash_recovery_code(&code);
    (code, hash)
}

/// Normalizes user input for a recovery code: uppercase, with dashes and spaces removed. Users may type it in lowercase
/// or without dashes.
pub fn normalize_recovery_code(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

pub fn hash_recovery_code(code: &str) -> [u8; 32] {
    Sha256::digest(normalize_recovery_code(code).as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_token_has_prefix_and_expected_length() {
        let (token, hash) = new_session_token();
        assert!(token.starts_with(TOKEN_PREFIX));
        assert_eq!(token.len(), 3 + 43);
        assert_eq!(hash_token(&token), hash);
    }

    #[test]
    fn session_tokens_do_not_repeat() {
        let tokens: std::collections::HashSet<String> = (0..1000).map(|_| new_session_token().0).collect();
        assert_eq!(tokens.len(), 1000);
    }

    #[test]
    fn recovery_code_has_four_groups_of_five_from_the_alphabet() {
        let (code, hash) = new_recovery_code();
        let groups: Vec<&str> = code.split('-').collect();
        assert_eq!(groups.len(), 4);
        assert!(groups.iter().all(|g| g.len() == 5));
        assert!(
            code.bytes()
                .filter(|b| *b != b'-')
                .all(|b| RECOVERY_ALPHABET.contains(&b))
        );
        assert_eq!(hash_recovery_code(&code), hash);
    }

    #[test]
    fn recovery_code_normalization_ignores_case_dashes_and_spaces() {
        let (code, hash) = new_recovery_code();
        let typed = code.to_lowercase().replace('-', " ");
        assert_eq!(hash_recovery_code(&typed), hash);
        assert_eq!(normalize_recovery_code("ab-cd ef"), "ABCDEF");
    }
}
