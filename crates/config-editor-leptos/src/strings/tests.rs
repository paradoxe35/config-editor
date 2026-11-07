use super::template::{counted, counted_as, fill, placeholders};
use super::*;

#[test]
fn placeholders_are_lowercase_names_in_braces() {
    assert_eq!(placeholders("{count} of {total}"), ["count", "total"]);
    assert_eq!(placeholders("Closes <{name}>"), ["name"]);
    assert_eq!(
        placeholders("{ not } {Count} {} {a-b} {x"),
        Vec::<&str>::new()
    );
    assert_eq!(placeholders("{{line}}"), ["line"]);
}

#[test]
fn fill_replaces_known_names_and_keeps_other_braces() {
    assert_eq!(
        fill(
            "{current} of {total}",
            &[("current", &2), ("total", &"12+")]
        ),
        "2 of 12+"
    );
    assert_eq!(
        fill("{a} {unknown} {", &[("a", &"x")]),
        "x {unknown} {",
        "an unknown placeholder is shown as written"
    );
    assert_eq!(fill("{a}{a}", &[("a", &1)]), "11");
    assert_eq!(fill("ünïcödé {a}…", &[("a", &"ok")]), "ünïcödé ok…");
}

/// Every count picks its singular text at 1 only, across every counted
/// pair of the defaults.
#[test]
fn counts_choose_singular_at_one_only() {
    let strings = EditorStrings::default();
    let pairs = [
        (
            &strings.errors_one,
            &strings.errors_other,
            "error",
            "errors",
        ),
        (
            &strings.warnings_one,
            &strings.warnings_other,
            "warning",
            "warnings",
        ),
        (&strings.hints_one, &strings.hints_other, "hint", "hints"),
        (
            &strings.changes_one,
            &strings.changes_other,
            "change",
            "changes",
        ),
        (&strings.lines_one, &strings.lines_other, "line", "lines"),
        (
            &strings.replaced_one,
            &strings.replaced_other,
            "occurrence",
            "occurrences",
        ),
    ];
    let mut failures = Vec::new();
    for (one, other, singular, plural) in pairs {
        for count in [0usize, 1, 2, 21] {
            let found = counted(one, other, count);
            let word = if count == 1 { singular } else { plural };
            if !found.ends_with(word) || !found.contains(&count.to_string()) {
                failures.push(format!("{count}: {found:?}, wanted {word}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert_eq!(counted_as("{count} a", "{count} b", 20, &"20+"), "20+ b");
}

#[test]
fn the_defaults_pass_their_own_check() {
    assert_eq!(EditorStrings::default().check(), []);
}

#[test]
fn an_unknown_placeholder_is_listed_with_what_the_text_takes() {
    let strings = EditorStrings {
        match_position: "{current}/{all}".into(),
        no_results: "{count} results".into(),
        // Leaving a placeholder out is fine.
        lines_other: "lines".into(),
        ..EditorStrings::default()
    };
    let problems = strings.check();
    let described: Vec<String> = problems.iter().map(ToString::to_string).collect();
    assert_eq!(
        described,
        [
            "strings.matchPosition: unknown placeholder {all} (this text takes {current}, {total})",
            "strings.noResults: unknown placeholder {count} (this text takes none)",
        ]
    );
}

#[test]
fn missing_keys_take_their_english_defaults() {
    let read: EditorStrings = serde_json::from_str(r#"{"noProblems": "Keine Fehler"}"#).unwrap();
    assert_eq!(read.no_problems, "Keine Fehler");
    assert_eq!(read.errors_one, EditorStrings::default().errors_one);
}

/// Every key, as JavaScript spells it, reads into its own field: a matrix
/// listing every key that does not.
#[test]
fn every_key_reads_from_camel_case_json() {
    let defaults = serde_json::to_value(EditorStrings::default()).unwrap();
    let keys: Vec<String> = defaults.as_object().unwrap().keys().cloned().collect();
    let failures: Vec<String> = keys
        .iter()
        .filter_map(|key| {
            let marked = format!("«{key}»");
            let read: EditorStrings = serde_json::from_value(serde_json::json!({ key: marked }))
                .map_err(|error| format!("{key}: {error}"))
                .ok()?;
            let written = serde_json::to_value(&read).unwrap();
            let changed: Vec<&String> = keys
                .iter()
                .filter(|other| written[other.as_str()] != defaults[other.as_str()])
                .collect();
            (changed != [key]).then(|| format!("{key}: changed {changed:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{failures:#?}");
    assert!(keys.len() > 100, "{}", keys.len());
}

#[test]
fn a_text_must_be_a_string() {
    let error = serde_json::from_str::<EditorStrings>(r#"{"find": 3}"#).unwrap_err();
    assert!(error.to_string().contains("expected a string"), "{error}");
}
