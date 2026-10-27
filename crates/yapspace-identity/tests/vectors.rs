//! Runs every entry in spec/url-identity/vectors.json against the identity crate.

use std::collections::HashSet;

use serde_json::Value;
use yapspace_identity::candidates;

const VECTORS: &str = include_str!("../../../spec/url-identity/vectors.json");

#[test]
fn every_vector_matches_its_expectation() {
    let vectors: Vec<Value> = serde_json::from_str(VECTORS).expect("vectors.json is valid JSON");
    assert!(!vectors.is_empty());

    let mut names = HashSet::new();
    let mut failures = Vec::new();
    for vector in &vectors {
        let name = vector["name"].as_str().expect("name");
        assert!(names.insert(name.to_owned()), "duplicate vector name: {name}");

        let observed = vector["observed"].as_str().expect("observed");
        let canonical = vector.get("canonical").and_then(Value::as_str);
        let expect = &vector["expect"];
        let result = candidates(observed, canonical, &[]);

        if let Some(reason) = expect.get("error").and_then(Value::as_str) {
            match &result {
                Err(actual) if actual.as_str() == reason => {}
                other => failures.push(format!("{name}: expected error {reason}, got {other:?}")),
            }
            continue;
        }

        let list = match result {
            Ok(list) => list,
            Err(err) => {
                failures.push(format!("{name}: expected a key, got error {err}"));
                continue;
            }
        };
        if let Some(key) = expect.get("key").and_then(Value::as_str) {
            if list[0].key != key {
                failures.push(format!("{name}: key {:?} != {key:?}", list[0].key));
            }
        }
        if let Some(display) = expect.get("display_url").and_then(Value::as_str) {
            if list[0].display_url != display {
                failures.push(format!(
                    "{name}: display {:?} != {display:?}",
                    list[0].display_url
                ));
            }
        }
        if let Some(expected) = expect.get("candidates").and_then(Value::as_array) {
            let actual: Vec<&str> = list.iter().map(|c| c.key.as_str()).collect();
            let expected: Vec<&str> = expected.iter().map(|v| v.as_str().expect("string")).collect();
            if actual != expected {
                failures.push(format!("{name}: candidates {actual:?} != {expected:?}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} vector(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
