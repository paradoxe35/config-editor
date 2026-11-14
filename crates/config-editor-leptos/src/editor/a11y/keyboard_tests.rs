//! The keys table against the READMEs and `aria-keyshortcuts`: the crate
//! README's Keyboard table is written from `KEYS` (regenerate with
//! `UPDATE_GENERATED=1 cargo test -p config-editor-leptos --lib keyboard`),
//! the npm README's key list names the same single chords, and the
//! textarea's attribute names every one of them that works in the text,
//! each while its setting is on.

use super::keyboard::{CONTROL_ONLY, KEYS, When, aria_chords, chords, gated_chords};
use super::shortcuts::{Context, chords_in};

const MARKER: &str = "keyboard";

fn repo_file(path: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

/// A Markdown table padded as Prettier writes it.
fn aligned(rows: &[[String; 2]]) -> String {
    let width = |text: &str| text.chars().count();
    let widths: Vec<usize> = (0..2)
        .map(|column| {
            rows.iter()
                .map(|row| width(&row[column]))
                .max()
                .unwrap_or(0)
                .max(3)
        })
        .collect();
    let line = |cells: [String; 2]| {
        let padded: Vec<String> = cells
            .iter()
            .enumerate()
            .map(|(column, cell)| format!("{cell}{}", " ".repeat(widths[column] - width(cell))))
            .collect();
        format!("| {} |", padded.join(" | "))
    };
    let mut out = vec![
        line(rows[0].clone()),
        line([0, 1].map(|column| "-".repeat(widths[column]))),
    ];
    out.extend(rows[1..].iter().cloned().map(line));
    out.join("\n")
}

/// The README's part between the markers.
fn generated() -> String {
    let mut rows = vec![["Keys".to_string(), "Action".to_string()]];
    rows.extend(
        KEYS.iter()
            .map(|row| [row.keys.to_string(), row.action.to_string()]),
    );
    let control_only: Vec<String> = CONTROL_ONLY
        .iter()
        .map(|chord| format!("`{chord}`"))
        .collect();
    let (last, rest) = control_only.split_last().expect("Ctrl-only chords");
    format!(
        "<!-- Generated from crates/config-editor-leptos/src/editor/a11y/keyboard.rs by its tests; do not edit. Regenerate with `UPDATE_GENERATED=1 cargo test -p config-editor-leptos --lib keyboard`. -->\n\n{}\n\n`Cmd` works in place of `Ctrl` on macOS, except in {} and {last}.\n\n",
        aligned(&rows),
        rest.join(", "),
    )
}

#[test]
fn the_readme_keyboard_table_is_written_from_the_keys() {
    let path = repo_file("crates/config-editor-leptos/README.md");
    let text = std::fs::read_to_string(&path).unwrap();
    let start_marker = format!("<!-- {MARKER}:start -->\n");
    let end_marker = format!("<!-- {MARKER}:end -->");
    let start = text.find(&start_marker).expect("a keyboard:start marker") + start_marker.len();
    let end = text[start..]
        .find(&end_marker)
        .expect("a keyboard:end marker")
        + start;
    let updated = format!("{}{}{}", &text[..start], generated(), &text[end..]);
    if updated != text {
        if std::env::var_os("UPDATE_GENERATED").is_some() {
            std::fs::write(&path, updated).unwrap();
        } else {
            panic!(
                "the README's Keyboard table is out of date: run `UPDATE_GENERATED=1 cargo test -p config-editor-leptos --lib keyboard`"
            );
        }
    }
}

#[test]
fn single_chords_are_the_keys_with_ctrl_or_alt_and_the_function_keys() {
    assert_eq!(
        chords("`Ctrl+Z`, `Ctrl+Shift+Z` / `Ctrl+Y`"),
        ["Ctrl+Z", "Ctrl+Shift+Z", "Ctrl+Y"]
    );
    assert_eq!(
        chords("`Enter` / `Shift+Enter`, `F3` / `Shift+F3`"),
        ["F3", "Shift+F3"]
    );
    assert!(chords("`Ctrl+K Ctrl+I` again").is_empty());
    assert!(chords("`Tab` / `Shift+Tab`").is_empty());
    assert_eq!(aria_chords("Shift+Alt+↑"), ["Shift+Alt+ArrowUp"]);
    assert_eq!(
        aria_chords("Ctrl+Shift+["),
        ["Control+Shift+[", "Meta+Shift+["]
    );
    assert_eq!(aria_chords("Ctrl+Space"), ["Control+Space"]);
}

/// Everything on.
fn all_on() -> Context {
    Context {
        editable: true,
        suggestions: true,
        find: true,
        go_to_line: true,
        save: true,
        problems: true,
        changes: true,
        folding: true,
        notice: true,
        documents: true,
    }
}

/// `context` with the setting `when` stands for turned off.
fn without(when: When) -> Context {
    let mut context = all_on();
    match when {
        When::Editable => context.editable = false,
        When::Suggestions => context.suggestions = false,
        When::Find | When::Replace => context.find = false,
        When::GoToLine => context.go_to_line = false,
        When::Save => context.save = false,
        When::Problems => context.problems = false,
        When::Changes => context.changes = false,
        When::Folding => context.folding = false,
        When::Notice => context.notice = false,
        When::Documents => context.documents = false,
        When::Always | When::Elsewhere => {}
    }
    context
}

#[test]
fn every_documented_single_chord_is_named_while_its_setting_is_on() {
    let named = chords_in(all_on());
    let named: Vec<&str> = named.split(' ').collect();
    for (chord, when) in gated_chords() {
        for spelled in aria_chords(chord) {
            if when == When::Elsewhere {
                assert!(
                    !named.contains(&spelled.as_str()),
                    "{chord} works elsewhere, not in the text"
                );
                continue;
            }
            assert!(
                named.contains(&spelled.as_str()),
                "{spelled} ({chord}) is not named"
            );
            if when != When::Always {
                let off = chords_in(without(when));
                assert!(
                    !off.split(' ').any(|named| named == spelled),
                    "{spelled} named with {when:?} off"
                );
            }
        }
    }
    // The ones QA 31 found missing (A31-2), among them.
    for spelled in [
        "Control+Shift+[",
        "Control+/",
        "Control+Shift+K",
        "Control+]",
        "Control+Enter",
        "Alt+ArrowUp",
    ] {
        assert!(named.contains(&spelled), "{spelled}");
    }
}

#[test]
fn the_npm_readme_names_the_same_single_chords() {
    let text = std::fs::read_to_string(repo_file("js/config-editor/README.md")).unwrap();
    let start = text.find("\n## Keyboard\n").expect("a Keyboard section");
    let end = text[start + 1..]
        .find("\n## ")
        .map_or(text.len(), |end| end + start + 1);
    let section = &text[start..end];
    let mut documented: Vec<&str> = section.lines().flat_map(chords).collect();
    documented.sort_unstable();
    documented.dedup();
    let mut table: Vec<&str> = KEYS.iter().flat_map(|row| chords(row.keys)).collect();
    table.sort_unstable();
    table.dedup();
    assert_eq!(
        documented, table,
        "the npm README's keys and the table differ"
    );
}
