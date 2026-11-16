//! QA `E36-1`/`E36-2`: a key cut in an Edit-as view and pasted back, at
//! once, after a pause, a switch to another view or to the host's
//! language, in its own block or another, keeps the form its host writes
//! it in that no view shows (an HCL expression or heredoc, a TOML
//! datetime or hex number, a YAML tag or block scalar, XML attributes),
//! in every host and view; a JSON view's cut of a block's last key, which
//! leaves a comma the text no longer reads with, is followed too.

use super::super::bridge::Bridge;
use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::sync::{convert_now, switch_to};
use super::super::test_state::TestEditor;
use crate::editor::save_matrix::report;
use leptos::prelude::*;

/// A key `sub` its host writes in a form of its own: the host's entry,
/// and the text the save keeps of it.
struct Kind {
    host: Host,
    entry: &'static str,
    kept: &'static [&'static str],
}

const KINDS: &[Kind] = &[
    Kind {
        host: Host::Hcl,
        entry: "sub = aws_subnet.main.id",
        kept: &["= aws_subnet.main.id\n"],
    },
    Kind {
        host: Host::Hcl,
        entry: "sub = upper(var.name)",
        kept: &["= upper(var.name)\n"],
    },
    Kind {
        host: Host::Hcl,
        entry: "sub = <<EOT\nhello\nEOT",
        kept: &["= <<EOT\nhello\nEOT\n"],
    },
    Kind {
        host: Host::Hcl,
        entry: "sub = \"web-${var.env}\"",
        kept: &["= \"web-${var.env}\"\n"],
    },
    Kind {
        host: Host::Toml,
        entry: "sub = 2024-01-01T00:00:00Z",
        kept: &["= 2024-01-01T00:00:00Z\n"],
    },
    Kind {
        host: Host::Toml,
        entry: "sub = 1979-05-27",
        kept: &["= 1979-05-27\n"],
    },
    Kind {
        host: Host::Toml,
        entry: "sub = 07:32:00",
        kept: &["= 07:32:00\n"],
    },
    Kind {
        host: Host::Toml,
        entry: "sub = 0x1F",
        kept: &["= 0x1F\n"],
    },
    Kind {
        host: Host::Yaml,
        entry: "sub: !Ref OwnerParam",
        kept: &[": !Ref OwnerParam\n"],
    },
    Kind {
        host: Host::Yaml,
        entry: "sub: !Sub \"${AWS::StackName}-data\"",
        kept: &[": !Sub \"${AWS::StackName}-data\"\n"],
    },
    Kind {
        host: Host::Yaml,
        entry: "sub: 'quoted'",
        kept: &[": 'quoted'\n"],
    },
    Kind {
        host: Host::Xml,
        entry: "<sub xsi:type=\"File\" name=\"f\"/>",
        kept: &["<sub xsi:type=\"File\" name=\"f\"/>"],
    },
    // Its prefix declared on the element it is cut from.
    Kind {
        host: Host::Xml,
        entry: "<c:sub pkg=\"x\"/>",
        kept: &["<c:sub ", "pkg=\"x\"", "/>"],
    },
    Kind {
        host: Host::Xml,
        entry: "<sub kind=\"x\" name=\"f\"/>",
        kept: &["<sub kind=\"x\" name=\"f\"/>"],
    },
    Kind {
        host: Host::Xml,
        entry: "<sub xsi:nil=\"true\"/>",
        kept: &["<sub xsi:nil=\"true\"/>"],
    },
    Kind {
        host: Host::Xml,
        entry: "<sub><![CDATA[x < y]]></sub>",
        kept: &["<sub><![CDATA[x < y]]></sub>"],
    },
    Kind {
        host: Host::Hcl,
        entry: "sub = {\n  Name = var.name\n}",
        kept: &["sub = {\n", "Name = var.name\n"],
    },
    Kind {
        host: Host::Hcl,
        entry: "sub {\n  Name = var.name\n}",
        kept: &["sub {\n", "Name = var.name\n"],
    },
    Kind {
        host: Host::Toml,
        entry: "sub = 1979-05-27T07:32:00",
        kept: &["= 1979-05-27T07:32:00\n"],
    },
    Kind {
        host: Host::Toml,
        entry: "sub = 'C:\\dir'",
        kept: &["= 'C:\\dir'\n"],
    },
    Kind {
        host: Host::Toml,
        entry: "sub = { d = 1979-05-27 }",
        kept: &["d = 1979-05-27"],
    },
    Kind {
        host: Host::Yaml,
        entry: "sub: |\n  hello\n  world",
        kept: &[": |\n    hello\n    world\n"],
    },
    Kind {
        host: Host::Yaml,
        entry: "sub: &anchor 5",
        kept: &[": &anchor 5\n"],
    },
    Kind {
        host: Host::Yaml,
        entry: "sub:\n  x: !Ref Param\n  y: \"q\"",
        kept: &["x: !Ref Param\n", "y: \"q\"\n"],
    },
];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Host {
    Hcl,
    Toml,
    Yaml,
    Xml,
}

impl Host {
    fn language(self) -> Language {
        match self {
            Host::Hcl => Language::HCL,
            Host::Toml => Language::TOML,
            Host::Yaml => Language::YAML,
            Host::Xml => Language::XML,
        }
    }
}

/// Where the key is in its block `a`, beside `mid`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum At {
    First,
    Last,
}

/// Where it is pasted: in `a` beside `mid`, or in `b` beside `other`,
/// or over `other`'s line selected whole (QA `E38-1`, `E38-2`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Place {
    SameBlock,
    OtherBlock,
    OverOther,
}

/// What happens between the cut and the paste.
#[derive(Clone, Copy, Debug)]
enum Between {
    Nothing,
    /// A pause while the cut left the text unread (a JSON view's comma).
    Pause,
    /// The text made to read again, then a pause.
    FixedThenPause,
    /// Text typed that no view reads, a pause, and the text deleted.
    BrokenThenPause,
    OtherView,
    HostLanguage,
}

/// The host's text with the blocks' entries as listed (`None` is `sub`).
fn host_text(kind: &Kind, a: &[Option<&str>], b: &[Option<&str>]) -> String {
    let entry = |name: Option<&str>, value: &str| match name {
        None => kind.entry.to_string(),
        Some(name) => match kind.host {
            Host::Hcl | Host::Toml => format!("{name} = {value}"),
            Host::Yaml => format!("{name}: {value}"),
            _ => format!("<{name}>{value}</{name}>"),
        },
    };
    let block = |name: &str, entries: &[Option<&str>]| -> String {
        let value = |name: Option<&str>| if name == Some("mid") { "1" } else { "2" };
        match kind.host {
            Host::Hcl => {
                let body: String = entries
                    .iter()
                    .map(|at| format!("  {}\n", indented(&entry(*at, value(*at)), "  ")))
                    .collect();
                format!("{name} {{\n{body}}}\n")
            }
            Host::Toml => {
                let body: String = entries
                    .iter()
                    .map(|at| format!("{}\n", entry(*at, value(*at))))
                    .collect();
                format!("[{name}]\n{body}")
            }
            Host::Yaml => {
                let body: String = entries
                    .iter()
                    .map(|at| format!("  {}\n", indented(&entry(*at, value(*at)), "  ")))
                    .collect();
                format!("{name}:\n{body}")
            }
            _ => {
                let body: String = entries
                    .iter()
                    .map(|at| format!("    {}\n", entry(*at, value(*at))))
                    .collect();
                // A prefix the key takes is declared on its first block.
                let declared = if name == "a" && kind.entry.contains("<c:") {
                    " xmlns:c=\"urn:c\""
                } else {
                    ""
                };
                format!("  <{name}{declared}>\n{body}  </{name}>\n")
            }
        }
    };
    match kind.host {
        Host::Xml => format!(
            "<r xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\n{}{}</r>\n",
            block("a", a),
            block("b", b)
        ),
        Host::Toml => format!("{}\n{}", block("a", a), block("b", b)),
        _ => format!("{}{}", block("a", a), block("b", b)),
    }
}

/// `entry`'s lines after its first indented by `indent`, unless they
/// are a heredoc's (which reads its lines as written).
fn indented(entry: &str, indent: &str) -> String {
    if entry.contains("<<") {
        return entry.to_string();
    }
    entry.replace('\n', &format!("\n{indent}"))
}

pub(super) fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

pub(super) fn line_offset(text: &str, line: usize) -> usize {
    text.split_inclusive('\n').take(line).map(str::len).sum()
}

pub(super) fn pause(state: EditorState) {
    convert_now(
        state,
        state.doc().text.get_untracked(),
        state.sync().edit_language.get_untracked(),
    );
}

/// The first line of `text` (from 0) writing the key `name`, or `name`
/// with the prefix `c:`.
pub(super) fn key_line(text: &str, name: &str) -> usize {
    text.lines()
        .position(|line| {
            let header = line.trim_end().trim_end_matches(']').trim_end_matches('"');
            if line.starts_with('[')
                && header.ends_with(&format!(".{name}")) | header.ends_with(&format!(".\"c:{name}"))
            {
                return true;
            }
            let line = line.trim_start().trim_start_matches(['"', '<']);
            let line = line.strip_prefix("c:").unwrap_or(line);
            line.strip_prefix(name)
                .is_some_and(|rest| rest.starts_with(['"', '>', ' ', ':', '=', '/']))
        })
        .unwrap_or_else(|| panic!("no {name} in\n{text}"))
}

/// `text` with the commas a JSON view's lines need, if `view` is JSON:
/// the first one missing or left over, `None` when none is.
fn comma_fix(view: Language, text: &str) -> Option<(usize, usize, String)> {
    if view != Language::JSON {
        return None;
    }
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    for (at, line) in lines.iter().enumerate() {
        let code = line.trim_end();
        let next = lines[at + 1..]
            .iter()
            .map(|line| line.trim())
            .find(|line| !line.is_empty());
        let Some(next) = next else { continue };
        let end = line_offset(text, at) + code.len();
        if code.ends_with(',') && next.starts_with(['}', ']']) {
            return Some((end - 1, end, String::new()));
        }
        if !code.ends_with([',', '{', '[']) && next.starts_with('"') {
            return Some((end, end, ",".to_string()));
        }
    }
    None
}

/// The view's text made to read again, one comma at a time, as typed.
fn fix_commas(state: EditorState, view: Language) {
    while let Some((start, end, typed)) = comma_fix(view, &state.doc().text.get_untracked()) {
        let before = state.doc().text.get_untracked();
        let text = format!("{}{typed}{}", &before[..start], &before[end..]);
        let (from, to) = (units(&before[..start]), units(&before[..end]));
        let caret = from + units(&typed);
        state.commit(Edit::new_units(before, (from, to), text, caret));
    }
}

/// The lines of `sub` cut from the view, whose text without them is
/// `without` (as the view shows the host without the key), the commas
/// aside.
fn cut(state: EditorState, view: Language, without: &str) -> String {
    let before = state.doc().text.get_untracked();
    let first = key_line(&before, "sub");
    let read = |text: &str| view.behavior().parse(text).ok();
    let wanted = read(without);
    let count = (1..=before.lines().count() - first)
        .find(|count| {
            let (from, to) = (
                line_offset(&before, first),
                line_offset(&before, first + count),
            );
            let mut left = format!("{}{}", &before[..from], &before[to..]);
            while let Some((start, end, typed)) = comma_fix(view, &left) {
                left = format!("{}{typed}{}", &left[..start], &left[end..]);
            }
            read(&left).is_some() && read(&left) == wanted
        })
        .unwrap_or_else(|| panic!("no lines of sub in\n{before}"));
    let (from, to) = (
        line_offset(&before, first),
        line_offset(&before, first + count),
    );
    let lines = before[from..to].to_string();
    let text = format!("{}{}", &before[..from], &before[to..]);
    let caret = units(&before[..from]);
    state.commit(Edit::new_units(
        before,
        (caret, caret + units(&lines)),
        text,
        caret,
    ));
    lines
}

/// `lines` pasted on the line of `name` (`after` it: on the next line).
pub(super) fn paste(state: EditorState, lines: &str, name: &str, after: bool) {
    let before = state.doc().text.get_untracked();
    let line = key_line(&before, name) + usize::from(after);
    let at = line_offset(&before, line);
    let text = format!("{}{lines}{}", &before[..at], &before[at..]);
    let start = units(&before[..at]);
    state.commit(Edit::new_units(
        before,
        (start, start),
        text,
        start + units(lines),
    ));
}

/// `lines` pasted over the line of `name`, selected whole (QA `E38-1`).
fn paste_over(state: EditorState, lines: &str, name: &str) {
    let before = state.doc().text.get_untracked();
    let line = key_line(&before, name);
    let (from, to) = (line_offset(&before, line), line_offset(&before, line + 1));
    let text = format!("{}{lines}{}", &before[..from], &before[to..]);
    let start = units(&before[..from]);
    state.commit(Edit::new_units(
        before.clone(),
        (start, units(&before[..to])),
        text,
        start + units(lines),
    ));
}

/// `lines` pasted at the end, after a blank line.
fn paste_at_end(state: EditorState, lines: &str) {
    let before = state.doc().text.get_untracked();
    let inserted = format!("\n{}", lines.trim_end());
    let text = format!("{}{inserted}\n", before.trim_end());
    let start = units(before.trim_end());
    state.commit(Edit::new_units(
        before,
        (start, start),
        text,
        start + units(&inserted),
    ));
}

fn other_view(host: Language, view: Language) -> Language {
    [Language::YAML, Language::JSON, Language::TOML]
        .into_iter()
        .find(|other| *other != host && *other != view)
        .expect("a third view")
}

/// The saved host's text for `kind` seen as `view`, the key cut at `at`,
/// `step` between, pasted at `place`; `Err` with what went wrong.
fn moved(kind: &Kind, view: Language, at: At, step: Between, place: Place) -> Result<(), String> {
    let over = place == Place::OverOther;
    let others: &[Option<&str>] = if over {
        &[Some("other"), Some("tail")]
    } else {
        &[Some("other")]
    };
    let (a, b): (Vec<Option<&str>>, Vec<Option<&str>>) = match at {
        At::First => (vec![None, Some("mid")], others.to_vec()),
        At::Last => (vec![Some("mid"), None], others.to_vec()),
    };
    let host = host_text(kind, &a, &b);
    let without = host_text(kind, &[Some("mid")], others);
    let bridge = Bridge::new(kind.host.language(), view, None).expect("two languages");
    let Ok(shown) = bridge.to_edit(&without) else {
        return Ok(());
    };
    let mut editor = TestEditor::new(kind.host.language(), &host);
    editor.edit_as = Some(view);
    let (_owner, state) = editor.mount();
    let lines = cut(state, view, &shown.text);
    match step {
        Between::Nothing => fix_commas(state, view),
        Between::Pause => {
            pause(state);
            fix_commas(state, view);
        }
        Between::FixedThenPause => {
            fix_commas(state, view);
            pause(state);
        }
        Between::BrokenThenPause => {
            fix_commas(state, view);
            let before = state.doc().text.get_untracked();
            let at = line_offset(&before, key_line(&before, "mid"));
            let broken = format!("{}[<\n{}", &before[..at], &before[at..]);
            let caret = units(&before[..at]);
            state.commit(Edit::new_units(
                before.clone(),
                (caret, caret),
                broken.clone(),
                caret + 3,
            ));
            pause(state);
            state.commit(Edit::new_units(broken, (caret, caret + 3), before, caret));
        }
        Between::OtherView => {
            fix_commas(state, view);
            switch_to(state, other_view(kind.host.language(), view));
            pause(state);
            switch_to(state, view);
        }
        Between::HostLanguage => {
            fix_commas(state, view);
            switch_to(state, kind.host.language());
            pause(state);
            switch_to(state, view);
        }
    }
    // Pasted where the key was not: past `mid` when it was before it.
    let (name, after) = match (place, at) {
        (Place::SameBlock, At::First) => ("mid", true),
        (Place::SameBlock, At::Last) => ("mid", false),
        (Place::OtherBlock | Place::OverOther, At::First) => ("other", false),
        (Place::OtherBlock | Place::OverOther, At::Last) => ("other", true),
    };
    // A TOML table goes after the keys, its header naming its block.
    if over {
        if lines.starts_with('[') {
            return Ok(());
        }
        paste_over(state, &lines, "other");
    } else if lines.starts_with('[') {
        paste_at_end(state, &lines);
        // Its header then made to name the other block, as typed.
        if place == Place::OtherBlock {
            let text = state.doc().text.get_untracked();
            let header = text
                .rfind(lines.lines().next().expect("a header"))
                .expect("pasted");
            let block = header + text[header..].find("a.").expect("the block");
            let before = text.clone();
            let after = format!("{}b{}", &text[..block], &text[block + 1..]);
            let at = units(&text[..block]);
            state.commit(Edit::new_units(before, (at, at + 1), after, at + 1));
        }
    } else {
        paste(state, &lines, name, after);
    }
    fix_commas(state, view);
    pause(state);
    let saved = state.sync().host_text.get_value();
    let expected = match (place, after) {
        (Place::OverOther, _) => host_text(kind, &[Some("mid")], &[None, Some("tail")]),
        (Place::SameBlock, true) => host_text(kind, &[Some("mid"), None], &[Some("other")]),
        (Place::SameBlock, false) => host_text(kind, &[None, Some("mid")], &[Some("other")]),
        (Place::OtherBlock, false) => host_text(kind, &[Some("mid")], &[None, Some("other")]),
        (Place::OtherBlock, true) => host_text(kind, &[Some("mid")], &[Some("other"), None]),
    };
    // A prefix's declaration goes with the element moved out of the one
    // declaring it: the data aside from where it is declared.
    let read = |text: &str| {
        let text =
            text.replace(" xmlns:c=\"urn:c\"", "")
                .replacen("<r ", "<r xmlns:c=\"urn:c\" ", 1);
        kind.host.language().behavior().parse(&text).ok()
    };
    let in_sync = state.sync().in_sync.get_untracked();
    // A prefixed element moved among its siblings in one save (none took
    // between) may wait, as it does from the host's language
    // (`key_trail_switches`).
    let reordered = place == Place::SameBlock && kind.entry.contains("<c:");
    if !in_sync && reordered && saved == host {
        return Ok(());
    }
    if !in_sync {
        let problems = state.sync().conversion_problems.get_untracked();
        return Err(format!(
            "the save waits ({problems:?}); view:\n{}",
            state.doc().text.get_untracked()
        ));
    }
    if read(&saved) != read(&expected)
        || kind
            .kept
            .iter()
            .any(|kept| saved.matches(kept).count() != 1)
    {
        return Err(format!("saved:\n{saved}"));
    }
    Ok(())
}

#[test]
fn a_key_cut_and_pasted_keeps_its_hosts_form_in_every_view() {
    let mut failures = Vec::new();
    for kind in KINDS {
        let views = [
            Language::YAML,
            Language::JSON,
            Language::TOML,
            Language::HCL,
            Language::XML,
        ];
        for view in views
            .into_iter()
            .filter(|view| *view != kind.host.language())
        {
            for at in [At::First, At::Last] {
                for step in [
                    Between::Nothing,
                    Between::Pause,
                    Between::FixedThenPause,
                    Between::BrokenThenPause,
                    Between::OtherView,
                    Between::HostLanguage,
                ] {
                    for place in [Place::SameBlock, Place::OtherBlock, Place::OverOther] {
                        if let Err(error) = moved(kind, view, at, step, place) {
                            failures.push(format!(
                                "{:?} `{}` as {view:?}, {at:?}, {step:?}, {place:?}: {error}",
                                kind.host.language(),
                                kind.entry
                            ));
                        }
                    }
                }
            }
        }
    }
    report(failures);
}
