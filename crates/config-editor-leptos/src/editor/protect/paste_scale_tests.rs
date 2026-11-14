//! A large paste into a text with many host ranges, in every language: the
//! lines it wrote are read for their paths in one pass of the text, and
//! each is weighed only against the locks its path relates it to, so the
//! work grows with the text and the locks, never with their product
//! (QA 33: 1,000 lines pasted into a text with 300 ranges froze it for
//! 10 to 16 s, each line's path a walk from the top, each line weighed
//! against each lock).

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::paths::{TextPosition, TextRange};
use super::super::test_state::TestEditor;
use super::super::work_count::{Work, counted_all};

/// `blocks` services, each with a host, a port, an object written inline
/// and a list over lines (the block of `range_scale_tests`).
fn text(language: Language, blocks: usize) -> String {
    let each = |index: usize| match language.label() {
        "YAML" => format!(
            "svc_{index}:\n  host: h{index}\n  port: {index}\n  opts: {{retries: {index}, wait: 2}}\n  tags: [\n    a,\n    b\n  ]\n"
        ),
        "JSON" => format!(
            "  \"svc_{index}\": {{\n    \"host\": \"h{index}\",\n    \"port\": {index},\n    \"opts\": {{ \"retries\": {index}, \"wait\": 2 }},\n    \"tags\": [\n      \"a\",\n      \"b\"\n    ]\n  }}{}\n",
            if index + 1 == blocks { "" } else { "," }
        ),
        "TOML" => format!(
            "[svc_{index}]\nhost = \"h{index}\"\nport = {index}\nopts = {{ retries = {index}, wait = 2 }}\ntags = [\n  \"a\",\n  \"b\"\n]\n"
        ),
        "HCL" => format!(
            "svc_{index} {{\n  host = \"h{index}\"\n  port = {index}\n  opts = {{ retries = {index}, wait = 2 }}\n  tags = [\n    \"a\",\n    \"b\"\n  ]\n}}\n"
        ),
        _ => format!(
            "  <svc_{index}>\n    <host>h{index}</host>\n    <port>{index}</port>\n    <opts><retries>{index}</retries><wait>2</wait></opts>\n    <tags>a</tags>\n    <tags>b</tags>\n  </svc_{index}>\n"
        ),
    };
    let body: String = (0..blocks).map(each).collect();
    match language.label() {
        "JSON" => format!("{{\n{body}}}\n"),
        "XML" => format!("<root>\n{body}</root>\n"),
        _ => body,
    }
}

/// The path walks and the lines weighed against a lock that pasting a
/// copy of the middle half of `text` (cut inside a block, so the text no
/// longer parses) costs, with a host range on each of the first `ranges`
/// host lines.
fn paste_work(language: Language, text: &str, ranges: usize) -> (usize, usize) {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut editor = TestEditor::new(language, text);
    editor.read_only_ranges = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.contains("host"))
        .take(ranges)
        .map(|(index, line)| {
            TextRange::new(
                TextPosition::new(index + 1, 1),
                TextPosition::new(index + 1, line.len() + 1),
            )
        })
        .collect();
    let (_owner, state) = editor.mount();
    let start = lines.len() / 4 + 2;
    let copy: String = lines[start..start + lines.len() / 2]
        .iter()
        .map(|line| format!("{line}\n"))
        .collect();
    let at: usize = lines[..lines.len() * 7 / 8]
        .iter()
        .map(|line| line.len() + 1)
        .sum();
    let ((), counts) = counted_all(|| {
        let before = state.doc().text.get_untracked();
        let after = format!("{}{copy}{}", &before[..at], &before[at..]);
        let caret = at + copy.len();
        state.commit(Edit::input(
            before,
            (at, at),
            after,
            (caret, caret),
            "insertFromPaste".into(),
            0.0,
        ));
    });
    let count = |work: Work| {
        counts[Work::ALL
            .iter()
            .position(|known| *known == work)
            .unwrap_or_default()]
    };
    (count(Work::PathLookup), count(Work::LockLine))
}

#[test]
fn a_large_paste_into_many_host_ranges_costs_work_linear_in_the_text_and_the_locks() {
    let mut slow = Vec::new();
    for language in Language::ALL.iter().copied() {
        for blocks in [150, 300] {
            let text = text(language, blocks);
            let lines = text.split('\n').count();
            for ranges in [blocks / 10, blocks] {
                let (walks, weighed) = paste_work(language, &text, ranges);
                // A few walks from the top for the whole paste; each line
                // weighed against the few locks its path is related to.
                if walks > 12 {
                    slow.push(format!(
                        "{language}, {blocks} blocks, {ranges} ranges: {walks} walks from the top"
                    ));
                }
                if weighed > 4 * (lines + ranges) {
                    slow.push(format!(
                        "{language}, {blocks} blocks, {ranges} ranges: {weighed} lines weighed against a lock for {lines} lines"
                    ));
                }
            }
        }
    }
    assert!(slow.is_empty(), "{slow:#?}");
}
