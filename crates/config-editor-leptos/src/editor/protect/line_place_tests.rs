//! A line command is judged as typing is (the check is one): a locked line
//! moved into the list below it changes where the field is, and is refused
//! in every language whose lists open on a line of their own, the text
//! parsing or not (it used to go through while the text did not parse);
//! moved past the whole list at once (a cut and paste), it is a reorder
//! inside its parent, let through (it used to be refused: no line diff keeps
//! a line moved past several).

use super::super::Language;
use super::super::keys::line_edit::{Folded, LineCommand, line_edit};
use super::super::paths::FieldPath;
use super::guard::check;
use super::locks::{Sources, protect};

/// A locked `image` followed by a list written over lines, in `language`,
/// the lock's path, and the list's opening line.
fn host(language: Language) -> (&'static str, &'static str, &'static str) {
    match language.label() {
        "YAML" => (
            "services:\n  web:\n    image: \"nginx:1.25\"\n    ports:\n      - \"80:80\"\n    restart: always\n",
            "/services/web/image",
            "    ports:",
        ),
        "JSON" => (
            "{\n  \"web\": {\n    \"image\": \"nginx\",\n    \"ports\": [\n      80\n    ],\n    \"restart\": \"always\"\n  }\n}\n",
            "/web/image",
            "    \"ports\"",
        ),
        "TOML" => (
            "[web]\nimage = \"nginx\"\nports = [\n  80,\n]\nrestart = \"always\"\n",
            "/web/image",
            "ports",
        ),
        "HCL" => (
            "web {\n  image = \"nginx\"\n  ports = [\n    80,\n  ]\n  restart = \"always\"\n}\n",
            "/web/image",
            "  ports",
        ),
        _ => (
            "<root>\n  <web>\n    <image>nginx</image>\n    <ports>\n      <port>80</port>\n    </ports>\n  </web>\n</root>\n",
            "/root/web/image",
            "    <ports>",
        ),
    }
}

/// The line of `text` just above `list`.
fn locked_line(text: &str, list: &str) -> String {
    text[..text.find(list).expect("the list")]
        .lines()
        .last()
        .expect("the locked line")
        .to_string()
}

/// `text` with the line `line` moved down.
fn moved_down(text: &str, line: &str) -> String {
    run(text, line, LineCommand::Move { down: true })
}

/// `text` after `command` on the line starting `line`.
fn run(text: &str, line: &str, command: LineCommand) -> String {
    let at = text.find(&format!("\n{line}")).expect("the line") + 1;
    let at = text[..at].encode_utf16().count();
    let none = Folded {
        folds: &[],
        units: &[],
        neighbors: &[],
    };
    line_edit(text, (at, at), command, none)
        .expect("an edit")
        .text
}

/// Whether the edit from `before` to `after` is let through, `pointer`
/// locked.
fn allowed(language: Language, before: &str, after: &str, pointer: &str) -> bool {
    let behavior = language.behavior();
    let paths = [FieldPath::from(pointer)];
    let protected = protect(
        behavior,
        before,
        Sources {
            paths: &paths,
            ..Sources::default()
        },
    );
    assert_eq!(protected.locks.len(), 1, "{language}: locked");
    check(behavior, None, &protected, before, after).is_ok()
}

#[test]
fn a_locked_line_moved_into_the_list_below_it_is_refused_in_every_language() {
    let mut let_through = Vec::new();
    for &language in Language::ALL {
        // A YAML list's items are below the line opening it, not in it.
        if language.label() == "YAML" {
            continue;
        }
        let (text, pointer, list) = host(language);
        let moved = moved_down(text, &locked_line(text, list));
        let parses = language.behavior().parse(&moved).is_ok();
        if allowed(language, text, &moved, pointer) {
            let_through.push(format!("{language} (parses: {parses}):\n{moved}"));
        }
    }
    assert!(let_through.is_empty(), "{}", let_through.join("\n"));
}

#[test]
fn a_locked_line_moved_past_the_list_below_it_is_let_through_in_every_language() {
    let mut refused = Vec::new();
    for &language in Language::ALL {
        let behavior = language.behavior();
        let (text, pointer, list) = host(language);
        let locked = locked_line(text, list);
        let document = behavior.parse(text).expect("parses");
        // Moved until the document reads as it did: past the list.
        let mut moved = moved_down(text, &locked);
        while behavior.parse(&moved).ok().as_ref() != Some(&document) {
            moved = moved_down(&moved, &locked);
        }
        if !allowed(language, text, &moved, pointer) {
            refused.push(format!("{language}:\n{moved}"));
        }
    }
    assert!(refused.is_empty(), "{}", refused.join("\n"));
}

/// YAML: a list's items left under the locked key (it moved onto them,
/// their key moved above it, or deleted) would join the locked value once
/// the text parses: refused, whichever command makes it (they went
/// through, the text then broken).
#[test]
fn a_list_left_under_a_locked_yaml_key_is_refused_whichever_command_makes_it() {
    let (text, pointer, _) = host(Language::YAML);
    let (image, ports) = ("    image:", "    ports:");
    let cases = [
        (
            "the locked line moved onto the items",
            run(text, image, LineCommand::Move { down: true }),
        ),
        (
            "the list's key moved above it",
            run(text, ports, LineCommand::Move { down: false }),
        ),
        (
            "the list's key deleted",
            run(text, ports, LineCommand::Delete),
        ),
    ];
    let let_through: Vec<String> = cases
        .iter()
        .filter(|(_, after)| allowed(Language::YAML, text, after, pointer))
        .map(|(name, after)| format!("{name}:\n{after}"))
        .collect();
    assert!(let_through.is_empty(), "{}", let_through.join("\n"));
}

/// JSON: the key holding the locked field moved above the document's
/// opening brace holds nothing there: refused (it went through, and the
/// broken text was saved).
#[test]
fn a_key_holding_a_lock_moved_out_of_its_object_is_refused() {
    let (text, pointer, _) = host(Language::JSON);
    let moved = run(text, "  \"web\"", LineCommand::Move { down: false });
    assert!(moved.starts_with("  \"web\": {\n{\n"), "{moved}");
    assert!(!allowed(Language::JSON, text, &moved, pointer), "{moved}");
}

/// YAML: a locked field on a list item's first line (`- id: 7`) stays
/// above the item's other keys: the line moved below one of them, or one
/// of them moved above it, leaves the item's keys outside it (it went
/// through, the text then broken). Its siblings reordered, or moved past
/// the item's end, stay free.
#[test]
fn a_locked_key_on_a_list_items_first_line_stays_above_its_keys() {
    let text = "list:\n  - id: 7\n    n: a\n    m: b\n  - n: c\n";
    let pointer = "/list/0/id";
    let down = LineCommand::Move { down: true };
    let up = LineCommand::Move { down: false };
    let refused = [
        (
            "the locked line moved below a key",
            run(text, "  - id", down),
        ),
        ("a key moved above it", run(text, "    n: a", up)),
    ];
    let mut failures: Vec<String> = refused
        .iter()
        .filter(|(_, after)| allowed(Language::YAML, text, after, pointer))
        .map(|(name, after)| format!("{name} let through:\n{after}"))
        .collect();
    let free = [
        ("its keys reordered", run(text, "    n: a", down)),
        (
            "its last key moved past the item",
            run(text, "    m: b", down),
        ),
    ];
    failures.extend(
        free.iter()
            .filter(|(_, after)| !allowed(Language::YAML, text, after, pointer))
            .map(|(name, after)| format!("{name} refused:\n{after}")),
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
