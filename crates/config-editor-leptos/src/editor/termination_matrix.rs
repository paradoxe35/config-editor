//! Every pass over the text ends: each language's constructs written over
//! lines (block scalars, quoted and plain scalars, flow collections,
//! multi-line strings, heredocs, block comments, CDATA, tags over lines),
//! with empty lines, lines of only blanks and lines at every indent put
//! inside them, go through the highlighter, folding, Enter at every line,
//! Edit as and the edit pipeline, each case under a time cap. A loop that
//! stops advancing (round 21: an empty line inside a YAML block scalar
//! froze the tab) fails here, naming the case, instead of hanging the
//! suite.

use std::sync::mpsc;
use std::time::Duration;

use leptos::prelude::*;

use super::completion::completions;
use super::handle::EditorHandle;
use super::highlight::spanning::{Spanning, tokens_after};
use super::keys::enter::{Pressed, enter_at};
use super::languages::Language;
use super::test_state::TestEditor;

const ALL: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

/// Generous for a debug build on a busy machine: a case takes milliseconds.
const CAP: Duration = Duration::from_secs(20);

/// Runs `work` on a thread of its own and fails, naming `case`, when it
/// does not finish within [`CAP`] (the stuck thread is left behind; the
/// test process ends with the suite).
#[track_caller]
fn within(case: String, work: impl FnOnce() + Send + 'static) {
    let (done, finished) = mpsc::channel();
    let named = case.clone();
    std::thread::Builder::new()
        .name(case.chars().take(60).collect())
        .stack_size(8 << 20)
        .spawn(move || {
            work();
            let _ = done.send(());
        })
        .expect("a test thread");
    match finished.recv_timeout(CAP) {
        Ok(()) => {}
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!("did not finish within {CAP:?} (a loop that never ends?): {named}")
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => panic!("panicked: {named}"),
    }
}

/// Each language's constructs over lines, the lines inside them (past the
/// first) being where blank and indented lines go.
fn constructs(language: Language) -> &'static [&'static str] {
    match language.label() {
        "YAML" => &[
            "s: |\n  a\n  b\nn: 1\n",
            "s: >\n  a\n  b\nn: 1\n",
            "s: >-\n  a\n  b\n",
            "s: |+\n  a\n  b\n",
            "s: |2\n   a\n  b\n",
            "m:\n  s: |\n    a\n    b\n  t: 1\n",
            "l:\n  - |\n    a\n    b\n  - x\n",
            "- >-\n  a\n  b\n",
            "s: \"a\n  b\n  c\"\nn: 1\n",
            "s: 'a\n  b\n  c'\n",
            "s: a\n  b\n  c\nn: 1\n",
            "l: [a,\n  b,\n  c]\n",
            "m: {a: 1,\n  b: 2}\n",
            "m:\n  a:\n    b: 1\n  c: 2\n",
            "- a: 1\n  b:\n    - x\n",
            "# c\ns: |\n  a # not a comment\n  b\n",
        ],
        "JSON" => &[
            "{\n  \"a\": [\n    1,\n    2\n  ],\n  \"b\": {\n    \"c\": \"x\"\n  }\n}\n",
            "[\n  \"a\",\n  \"b\"\n]\n",
        ],
        "TOML" => &[
            "s = \"\"\"\na\nb\n\"\"\"\nn = 1\n",
            "s = '''\na\nb\n'''\n",
            "s = \"\"\"a\\\n  b\"\"\"\n",
            "l = [\n  1,\n  2,\n]\n",
            "[t]\na = 1\nb = 2\n[u]\nc = 3\n",
            "[[a]]\nb = 1\n[[a]]\nb = 2\n",
            "# c\na = 1\n# d\n",
        ],
        "HCL" => &[
            "s = <<EOF\na\nb\nEOF\nn = 1\n",
            "s = <<-EOF\n  a\n  b\n  EOF\n",
            "/* a\nb\n*/\nn = 1\n",
            "b {\n  a = 1\n  c {\n    d = 2\n  }\n}\n",
            "l = [\n  1,\n  2,\n]\n",
            "o = {\n  a = 1\n  b = \"${x}\"\n}\n",
        ],
        _ => &[
            "<r>\n  <a>1</a>\n  <b>\n    <c>2</c>\n  </b>\n</r>\n",
            "<r>\n<!-- a\nb\n-->\n<a>1</a>\n</r>\n",
            "<r><![CDATA[a\nb\n]]></r>\n",
            "<r>\n  <a\n    x=\"1\"\n    y=\"2\">t</a>\n</r>\n",
            "<r>\n  <a>line one\n  line two</a>\n</r>\n",
            "<?xml version=\"1.0\"?>\n<r a=\"x\ny\"/>\n",
        ],
    }
}

/// The lines put inside a construct: empty, only blanks, and text at every
/// indent.
const INSERTED: &[&str] = &[
    "", " ", "  ", "   ", "    ", "\t", " \t ", "x", " x", "  x", "   x", "    x", "      x",
];

/// `text` with each [`INSERTED`] line added before, and put in place of,
/// each line past the first (and after the last one).
fn variants(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = vec![text.to_string()];
    for at in 1..lines.len() {
        for inserted in INSERTED {
            let mut added = lines.clone();
            added.insert(at, inserted);
            out.push(added.join("\n"));
            if at < lines.len() - 1 {
                let mut replaced = lines.clone();
                replaced[at] = inserted;
                out.push(replaced.join("\n"));
            }
        }
    }
    // Blank lines in a row, and a document ending without a line break.
    out.push(text.replace('\n', "\n\n"));
    out.push(text.replace('\n', "\n  \n"));
    out.push(text.trim_end().to_string());
    out
}

/// Every reading of `text` a language does: the ones the view, folding,
/// the keys and suggestions run on each edit, at every offset that takes
/// one.
fn read_all(language: Language, text: &str) {
    let behavior = language.behavior();
    let _ = behavior.parse(text);
    let comments = behavior.comments(text);
    let constructs = behavior.multiline_constructs(text);
    let spanning = Spanning::of(text, &comments, &constructs);
    for (index, line) in text.split('\n').enumerate() {
        let _ = behavior.highlight(line);
        let _ = tokens_after(language, line, spanning.at(index));
        let _ = behavior.highlight_continued(line);
        let _ = behavior.comment_start(line);
        for column in 0..=line.len() {
            let _ = behavior.hover_key(line, column);
            let _ = behavior.hover_path(text, index, column);
        }
    }
    let _ = behavior.fold_ranges(text);
    let _ = behavior.string_body_lines(text);
    let _ = behavior.saved_string_lines(text);
    let _ = behavior.multiline_strings(text);
    let _ = behavior.empty_value_slots(text);
    let _ = behavior.first_span(text);
    let units = text.encode_utf16().count();
    let _ = behavior.toggle_comments(text, 0, units);
    for cursor in 0..=units {
        let _ = behavior.newline(text, cursor);
        let _ = behavior.lexical_state(text, cursor);
        let _ = behavior.completion_context(text, cursor);
        let _ = completions(text, cursor, true, None, true, behavior);
        let _ = behavior.toggle_comments(text, cursor, cursor);
        for typed in ['"', '\'', '<', '>', '/', '{', '[', '$', ':', '-', '\n'] {
            let _ = behavior.auto_close(text, cursor, typed);
            let _ = behavior.reindent_on_type(text, cursor, typed);
        }
    }
}

/// What the view reads after each edit.
fn read_view(state: super::state::EditorState) {
    let _ = state.view().lines.get_untracked();
    let _ = state.view().visible_lines.get_untracked();
    let _ = state.view().spanning.get_untracked();
    let _ = state.view().string_bodies.get_untracked();
    #[cfg(feature = "folding")]
    let _ = state.fold().fold_ranges.get_untracked();
    let text = state.doc().text.get_untracked();
    state.problems().diagnosed_text.set(text);
    let _ = state.problems().diagnostics.get_untracked();
}

/// Mounts an editor on `text` (shown as `edit_as`), and presses Enter at
/// the start, middle and end of every line, the view read after each.
fn edit_all(language: Language, edit_as: Option<Language>, text: &str) {
    let (_owner, state) = TestEditor {
        edit_as,
        ..TestEditor::new(language, text)
    }
    .mount();
    let handle = EditorHandle::new();
    handle.attach(state);
    read_view(state);
    let shown = state.doc().text.get_untracked();
    let mut offsets = Vec::new();
    let mut start = 0;
    for line in shown.split('\n') {
        let units = line.encode_utf16().count();
        offsets.extend([start, start + units / 2, start + units]);
        start += units + 1;
    }
    for offset in offsets {
        let before = state.doc().text.get_untracked();
        enter_at(state, (offset, offset), Pressed::Enter);
        read_view(state);
        let _ = handle.value();
        enter_at(state, (offset, offset), Pressed::LineBelow);
        enter_at(state, (offset, offset), Pressed::LineAbove);
        read_view(state);
        let _ = handle.value();
        state.doc().text.set(before);
    }
}

#[test]
fn reading_constructs_with_blank_and_indented_lines_ends() {
    for language in ALL {
        for (sample, text) in constructs(language).iter().enumerate() {
            let cases = variants(text);
            within(
                format!("{language} sample {sample} read: {text:?}"),
                move || {
                    for case in cases {
                        read_all(language, &case);
                    }
                },
            );
        }
    }
}

#[test]
fn editing_constructs_with_blank_and_indented_lines_ends() {
    for language in ALL {
        for (sample, text) in constructs(language).iter().enumerate() {
            let cases = variants(text);
            within(
                format!("{language} sample {sample} edit: {text:?}"),
                move || {
                    for case in cases {
                        edit_all(language, None, &case);
                    }
                },
            );
        }
    }
}

/// Host strings holding line breaks, blank lines and lines of only blanks,
/// shown in every other language (YAML writes them as block scalars,
/// TOML as `"""`, HCL as heredocs), then edited there.
#[test]
fn editing_multi_line_host_strings_in_every_language_ends() {
    let values = [
        "a\n\nb",
        "a\n  \nb",
        "a\n\n\nb\n",
        "\n\na",
        "a\n",
        "  a\n\n  b",
        "a\n\t\nb",
    ];
    for host in ALL {
        for edit_as in ALL {
            if host == edit_as {
                continue;
            }
            for value in values {
                let text = host
                    .behavior()
                    .serialize(
                        &serde_json::json!({ "s": value, "m": { "t": value }, "l": [value] }),
                        None,
                    )
                    .expect("the host writes it");
                within(format!("{host} as {edit_as}: {value:?}"), move || {
                    edit_all(host, Some(edit_as), &text)
                });
            }
        }
    }
}
