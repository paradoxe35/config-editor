//! A locked line swapped with its neighbour (Alt+Up/Down, or a cut and
//! paste of the two lines) keeps the locked value as it was, whichever way
//! the two lines change places: each direction, and the swap back (its
//! undo), is judged alike, in every language.

use super::super::Language;
use super::super::paths::FieldPath;
use super::guard::check;
use super::locks::{Sources, protect};

/// Four sibling fields `a`..`d`, one a line, written in `language`, with
/// the lines of `a`, `b` and `c` in `order`; and the root they are under.
fn text(language: Language, order: [usize; 3]) -> (String, &'static str) {
    let names = ["a", "b", "c"];
    let line = |name: &str, value: usize| match language.label() {
        "YAML" => format!("{name}: {value}\n"),
        "JSON" => format!("  \"{name}\": {value},\n"),
        "TOML" | "HCL" => format!("{name} = {value}\n"),
        _ => format!("  <{name}>{value}</{name}>\n"),
    };
    let body: String = order
        .iter()
        .map(|at| line(names[*at], at + 1))
        .chain([match language.label() {
            "JSON" => "  \"d\": 4\n".to_string(),
            _ => line("d", 4),
        }])
        .collect();
    match language.label() {
        "JSON" => (format!("{{\n{body}}}\n"), ""),
        "XML" => (format!("<r>\n{body}</r>\n"), "/r"),
        _ => (body, ""),
    }
}

/// Whether the edit from the lines in `from` to the lines in `to` is let
/// through, `b` locked.
fn allowed(language: Language, from: [usize; 3], to: [usize; 3]) -> Result<(), String> {
    let (before, root) = text(language, from);
    let (after, _) = text(language, to);
    let paths = [FieldPath::from(format!("{root}/b").as_str())];
    let behavior = language.behavior();
    let protected = protect(
        behavior,
        &before,
        Sources {
            paths: &paths,
            ..Sources::default()
        },
    );
    assert_eq!(protected.locks.len(), 1, "{language}: {before}");
    check(behavior, None, &protected, &before, &after).map_err(|blocked| format!("{blocked:?}"))
}

#[test]
fn a_locked_line_swapped_with_a_neighbour_is_judged_alike_both_ways() {
    let mut refused = Vec::new();
    let straight = [0, 1, 2];
    // `b` moved up past `a`, and down past `c`.
    for swapped in [[1, 0, 2], [0, 2, 1]] {
        for &language in Language::ALL {
            for (from, to) in [(straight, swapped), (swapped, straight)] {
                if let Err(why) = allowed(language, from, to) {
                    refused.push(format!("{language}: {from:?} -> {to:?}: {why}"));
                }
            }
        }
    }
    assert!(refused.is_empty(), "{refused:#?}");
}
