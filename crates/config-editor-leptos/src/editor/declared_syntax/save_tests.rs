//! Edit-as saves of a language declared as a JavaScript host declares one,
//! with all its syntax (`itemMarker`, `mapBrackets`, `literalQuotes`) and
//! with only its comments and separator: list items move with their
//! comments, and strings keep their quotes, from every built-in view.

use serde_json::{Value, json};

use super::super::Language;
use super::super::save_matrix::{ALL, report, save_value};
use super::{DOT, DOT_BARE, DOT_ESCAPING};

fn languages() -> [Language; 2] {
    [Language::new(&DOT), Language::new(&DOT_BARE)]
}

const HOST: &str = "\
# DOT header

name: 'C:\\dir'   # literal backslash
port: 8080
tags: (a; b; c)  # one-line list
obj: <x: 1; y: 'two'>   # one-line map
items:
  # first item
  * <n: 1>
  # second item
  * <n: 2>
last: true
";

/// Each language and view saves `change` of `host` as `expected`.
fn check(
    failures: &mut Vec<String>,
    label: &str,
    host: &str,
    change: &dyn Fn(&mut Value),
    expected: &str,
) {
    check_in(&languages(), failures, label, host, change, expected);
}

fn check_in(
    languages: &[Language],
    failures: &mut Vec<String>,
    label: &str,
    host: &str,
    change: &dyn Fn(&mut Value),
    expected: &str,
) {
    for &language in languages {
        for edit in ALL {
            match save_value(language, edit, host, change) {
                Ok(saved) if saved == expected => {}
                Ok(saved) => failures.push(format!(
                    "{label}: {language} via {edit}:\n--- expected\n{expected}--- saved\n{saved}"
                )),
                Err(error) => failures.push(format!("{label}: {language} via {edit}: {error}")),
            }
        }
    }
}

fn set(pointer: &'static str, value: Value) -> impl Fn(&mut Value) {
    move |document: &mut Value| {
        if let Some(slot) = document.pointer_mut(pointer) {
            *slot = value.clone();
        }
    }
}

fn swap(pointer: &'static str) -> impl Fn(&mut Value) {
    move |document: &mut Value| {
        if let Some(Value::Array(items)) = document.pointer_mut(pointer) {
            items.swap(0, 1);
        }
    }
}

#[test]
fn list_items_moved_take_their_comments_along() {
    let mut failures = Vec::new();
    let hosts = [
        (
            HOST,
            HOST.replace(
                "  # first item\n  * <n: 1>\n  # second item\n  * <n: 2>\n",
                "  # second item\n  * <n: 2>\n  # first item\n  * <n: 1>\n",
            ),
        ),
        (
            "items:\n  # first item\n  * <n: 1>\n  # second item\n  * <n: 2>\n",
            "items:\n  # second item\n  * <n: 2>\n  # first item\n  * <n: 1>\n".to_string(),
        ),
        (
            "items:\n  * <n: 1>  # first\n  * <n: 2>  # second\nlast: true\n",
            "items:\n  * <n: 2>  # second\n  * <n: 1>  # first\nlast: true\n".to_string(),
        ),
    ];
    let rotate = |document: &mut Value| {
        if let Some(Value::Array(items)) = document.pointer_mut("/hosts") {
            items.rotate_left(1);
        }
    };
    check_in(
        &[Language::new(&DOT)],
        &mut failures,
        "scalar items rotated",
        "name: web\nhosts:\n  # first host\n  * alpha   # a\n  * beta    # b\n  # gamma below\n  * gamma\ntail: x\n",
        &rotate,
        "name: web\nhosts:\n  * beta    # b\n  # gamma below\n  * gamma\n  # first host\n  * alpha   # a\ntail: x\n",
    );
    for (host, expected) in &hosts {
        check_in(
            &[Language::new(&DOT)],
            &mut failures,
            "items swapped",
            host,
            &swap("/items"),
            expected,
        );
    }
    // A language that declares no item marker cannot say where an item's
    // lines are: the save waits rather than drop or misplace the comments.
    for (host, _) in &hosts {
        for edit in ALL {
            match save_value(Language::new(&DOT_BARE), edit, host, &swap("/items")) {
                Err(error) if error.contains("cannot be moved with their comments") => {}
                other => failures.push(format!(
                    "items swapped: DotBare via {edit} did not wait: {other:?}"
                )),
            }
        }
    }
    report(failures);
}

#[test]
fn a_string_edited_in_a_one_line_container_keeps_its_quotes() {
    let mut failures = Vec::new();
    check(
        &mut failures,
        "a map's string edited",
        HOST,
        &set("/obj/y", json!("three")),
        &HOST.replace("y: 'two'", "y: 'three'"),
    );
    check(
        &mut failures,
        "a map's string edited to hold a bracket",
        HOST,
        &set("/obj/y", json!("a>b")),
        &HOST.replace("y: 'two'", "y: 'a>b'"),
    );
    // Only where the language declares its lists' brackets is the list
    // edited in place (else its writer writes it again).
    check_in(
        &[Language::new(&DOT)],
        &mut failures,
        "a list's string edited",
        &HOST.replace("(a; b; c)", "('a'; 'b'; c)"),
        &set("/tags/1", json!("bee")),
        &HOST.replace("(a; b; c)", "('a'; 'bee'; c)"),
    );
    report(failures);
}

/// A value changed on a line with a comment after it keeps the comment
/// (the value ends where a declared comment starts after whitespace).
#[test]
fn a_value_changed_keeps_the_comment_after_it() {
    let mut failures = Vec::new();
    let host = "n: 5  # c5\nb: true # c6\ns: 'a # b'  # c7\n";
    let cases = [
        ("/n", json!(6), "n: 6  # c5"),
        ("/b", json!(false), "b: false # c6"),
        ("/s", json!("x"), "s: 'x'  # c7"),
    ];
    for (pointer, value, line) in cases {
        let expected = host
            .lines()
            .map(|old| {
                if old.starts_with(&pointer[1..]) {
                    line
                } else {
                    old
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        check(
            &mut failures,
            pointer,
            host,
            &set(pointer, value),
            &expected,
        );
    }
    report(failures);
}

/// A save never writes a key a second time, even where the declared quotes
/// misread a one-line list (a backslash before the closing quote, read as
/// an escape) and the language's parse lets a later line win.
#[test]
fn a_save_writes_no_key_twice() {
    let mut failures = Vec::new();
    let host = "l: ('x\\'; 3)  # d\nk: 1\n";
    for edit in ALL {
        let saved = save_value(
            Language::new(&DOT_ESCAPING),
            edit,
            host,
            &set("/l/1", json!(4)),
        );
        if let Ok(saved) = &saved
            && saved.lines().filter(|line| line.starts_with("l:")).count() > 1
        {
            failures.push(format!("via {edit}: {saved:?}"));
        }
    }
    report(failures);
}
