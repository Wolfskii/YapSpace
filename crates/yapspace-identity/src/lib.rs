//! Pure, versioned URL identity rules for YapSpace. No IO and no async.

/// Ruleset version embedded in every identity key (`v1|…`).
pub const RULESET: u16 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruleset_is_v1() {
        assert_eq!(RULESET, 1);
    }
}
