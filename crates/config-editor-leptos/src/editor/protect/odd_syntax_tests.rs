//! Read-only fields of the odd language (`;` comments, `:=`, `@` items,
//! `( … | … )` lists): located, kept and refused from what the trait says,
//! as in the built-in languages.

use std::ops::Range;

use super::super::Language;
use super::super::odd_syntax::ODD;
use super::guard::check;
use super::locks::{Protected, Sources, protect};
use super::tests::schema;

const TEXT: &str = "\
; odd
name := x
id := abc ; set by the server
meta :=
  a := 1
  b := 2
ports := (80 | 443)
port := 80
";

fn odd() -> Language {
    Language::new(&ODD)
}

fn locks() -> Protected {
    let schema = schema();
    protect(
        odd().behavior(),
        TEXT,
        Sources {
            marking_schema: Some(&schema),
            reading_schema: Some(&schema),
            ..Sources::default()
        },
    )
}

fn covered(range: &Range<usize>) -> String {
    String::from_utf16_lossy(&TEXT.encode_utf16().collect::<Vec<_>>()[range.clone()])
}

#[test]
fn odd_read_only_fields_are_located() {
    let protected = locks();
    let found: Vec<(String, Vec<String>)> = protected
        .locks
        .iter()
        .map(|lock| {
            (
                lock.pointer.clone().unwrap_or_default(),
                lock.ranges.iter().map(covered).collect(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            ("/id".to_string(), vec!["id := abc".to_string()]),
            (
                "/meta".to_string(),
                vec!["meta :=\n  a := 1\n  b := 2".to_string()]
            ),
        ]
    );
}

#[test]
fn odd_edits_of_a_read_only_field_are_refused_and_others_allowed() {
    let protected = locks();
    let schema = schema();
    let edits = [
        ("change the value", "id := abc", "id := abd", true),
        (
            "delete the line",
            "id := abc ; set by the server\n",
            "",
            true,
        ),
        ("rename the key", "id :=", "idx :=", true),
        ("comment it out", "id := abc", "; id := abc", true),
        ("change a value inside the block", "a := 1", "a := 9", true),
        (
            "add a child to the block",
            "  b := 2\n",
            "  b := 2\n  c := 3\n",
            true,
        ),
        ("change another value", "port := 80", "port := 81", false),
        (
            "change an item of a list",
            "(80 | 443)",
            "(80 | 8443)",
            false,
        ),
        (
            "change the comment",
            "; set by the server",
            "; set by hand",
            false,
        ),
        ("space the list", "(80 | 443)", "( 80 | 443 )", false),
    ];
    let mut failures = Vec::new();
    for (what, from, to, refused) in edits {
        let edited = TEXT.replacen(from, to, 1);
        assert_ne!(edited, TEXT, "{what}");
        let allowed = check(odd().behavior(), Some(&schema), &protected, TEXT, &edited).is_ok();
        if allowed == refused {
            failures.push(format!(
                "{what}: {}",
                if refused { "allowed" } else { "refused" }
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
