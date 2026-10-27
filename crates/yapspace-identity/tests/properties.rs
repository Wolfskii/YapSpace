//! Property tests: the normalizer never panics, and normalizing a display URL gives the same key again.

use proptest::prelude::*;
use yapspace_identity::{RejectReason, candidates};

proptest! {
    #[test]
    fn never_panics_on_arbitrary_input(input in ".*") {
        let _ = candidates(&input, None, &[]);
        let _ = candidates(&input, Some(&input), &[]);
    }

    #[test]
    fn display_url_normalizes_back_to_the_same_key(
        host in "[a-z]{3,8}",
        path in "(/[a-z0-9]{1,6}){0,3}",
        query in "([a-z]{1,4}=[a-z0-9]{0,4}(&[a-z]{1,4}=[a-z0-9]{0,4}){0,2})?",
    ) {
        let url = if query.is_empty() {
            format!("https://{host}.com{path}")
        } else {
            format!("https://{host}.com{path}?{query}")
        };
        if let Ok(first) = candidates(&url, None, &[]) {
            let again = candidates(&first[0].display_url, None, &[]).expect("display URL must be accepted");
            prop_assert_eq!(&again[0].key, &first[0].key);
        }
    }
}

#[test]
fn input_longer_than_2048_bytes_is_too_long() {
    let long = format!("https://example.com/{}", "a".repeat(2100));
    assert_eq!(candidates(&long, None, &[]).unwrap_err(), RejectReason::TooLong);
}
