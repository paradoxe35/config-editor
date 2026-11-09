//! Thousands of the host's secret placeholders, checked at each pause in
//! typing: each value and each problem is looked up among them, not
//! compared with every one (2000 secrets made each pause's check take
//! hundreds of milliseconds, growing with their square).

use super::super::Language;
use super::super::work_count::{Work, counted};
use super::{Schema, validate_with};
use crate::strings::default_strings;

/// `services` services, each with a placeholder for its secret `key` and
/// a `port` the schema says is a string (a problem each).
fn text(services: usize) -> String {
    let mut root = serde_json::Map::new();
    for index in 0..services {
        root.insert(
            format!("svc_{index}"),
            serde_json::json!({"key": format!("«secret {index:x}»"), "port": index}),
        );
    }
    Language::JSON
        .serialize(&serde_json::Value::Object(root))
        .expect("written")
}

/// The problems on `services` services, and the passes over their
/// placeholders checking them took.
fn checked(services: usize) -> (usize, usize) {
    let schema = Schema::compile(
        r#"{"additionalProperties": {"properties": {
            "key": {"type": "string", "writeOnly": true, "minLength": 40},
            "port": {"type": "string"}
        }}}"#,
    )
    .expect("compiles");
    let accepted: Vec<(String, String)> = (0..services)
        .map(|index| (format!("«secret {index:x}»"), format!("/svc_{index}/key")))
        .collect();
    let text = text(services);
    let (problems, passes) = counted(Work::TokenScan, || {
        validate_with(
            &text,
            Some(&schema.bundle),
            None,
            true,
            Language::JSON.behavior(),
            &accepted,
            default_strings(),
        )
    });
    // The placeholders pass (their `minLength` is not theirs to meet):
    // only the ports are problems.
    assert!(
        problems
            .iter()
            .all(|problem| !problem.message.contains("/key")),
        "{problems:#?}"
    );
    (problems.len(), passes)
}

#[test]
fn placeholders_are_looked_up_not_compared_with_each_value_and_problem() {
    let (few, few_passes) = checked(20);
    let (many, many_passes) = checked(200);
    assert!(few >= 20 && many >= few, "{few} and {many} problems");
    assert_eq!(
        few_passes, many_passes,
        "passes over the placeholders: {few_passes} for 20, {many_passes} for 200"
    );
}
