//! One host range locks a field in a long text, in every language: a line
//! typed far from it finds the fields the range covers in work linear in
//! the text, the text walked from the top a fixed number of times, never
//! once per value written inline (QA 31: HCL walked the text from the top
//! for each statement holding a key, 6.6 s per key at 15,000 lines).

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::paths::{TextPosition, TextRange};
use super::super::test_state::TestEditor;
use super::super::work_count::{Work, counted_all};

/// `blocks` services, each with a host, a port, an object written inline
/// and a list over lines.
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

/// The demo's large sample in `language` (about 2,000 lines).
fn sample(language: Language) -> &'static str {
    match language.label() {
        "YAML" => include_str!("../../../tests/fixtures/samples/tunnel.large.yaml"),
        "JSON" => include_str!("../../../tests/fixtures/samples/tunnel.large.json"),
        "TOML" => include_str!("../../../tests/fixtures/samples/tunnel.large.toml"),
        "HCL" => include_str!("../../../tests/fixtures/samples/tunnel.large.hcl"),
        _ => include_str!("../../../tests/fixtures/samples/tunnel.large.xml"),
    }
}

/// Each kind of work (by [`Work::ALL`]) that a line typed in the last
/// third of `text` costs (a comment where the language has one, else a
/// blank line), with a host range over the first line holding `locked`.
fn typing_work(language: Language, text: &str, locked: &str) -> Vec<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    let host = lines
        .iter()
        .position(|line| line.contains(locked))
        .expect("the locked line");
    let mut editor = TestEditor::new(language, text);
    editor.read_only_ranges = vec![TextRange::new(
        TextPosition::new(host + 1, 1),
        TextPosition::new(host + 1, lines[host].chars().count() + 1),
    )];
    let (_owner, state) = editor.mount();
    assert!(
        state
            .protect()
            .protected
            .with_untracked(|p| p.locks.iter().any(|lock| lock.pointer.is_some())),
        "{language}: the range locks its field"
    );
    let typed = match language.label() {
        "JSON" | "XML" => " \n",
        _ => "#\n",
    };
    let byte: usize = lines[..lines.len() * 2 / 3]
        .iter()
        .map(|line| line.len() + 1)
        .sum();
    let ((), counts) = counted_all(|| {
        let before = state.doc().text.get_untracked();
        let at = before[..byte].encode_utf16().count();
        let after = format!("{}{typed}{}", &before[..byte], &before[byte..]);
        let caret = at + typed.len();
        let key = Edit::input(
            before,
            (at, at),
            after,
            (caret, caret),
            "insertFromPaste".into(),
            0.0,
        );
        assert!(state.commit(key), "{language}: typed");
        let _ = state.protect().protected.with_untracked(|p| p.locks.len());
    });
    counts.to_vec()
}

/// The index of `work` in [`Work::ALL`].
fn kind(work: Work) -> usize {
    Work::ALL
        .iter()
        .position(|known| *known == work)
        .unwrap_or_default()
}

#[test]
fn a_line_typed_far_from_a_host_range_finds_its_fields_in_linear_work() {
    let mut slow = Vec::new();
    for language in Language::ALL.iter().copied() {
        let small = typing_work(language, &text(language, 200), "h0");
        let large = typing_work(language, &text(language, 800), "h0");
        for (index, work) in Work::ALL.iter().enumerate() {
            // Walks from the top and reads of the whole document stay as
            // many; the rest grows with the text (linear is 4 times, work
            // per field for each field 16).
            let fixed = matches!(work, Work::PathLookup | Work::DocumentRead);
            let over = if fixed {
                large[index] > small[index] + 2
            } else {
                large[index] > 6 * small[index].max(200)
            };
            if over {
                slow.push(format!(
                    "{language}: {work:?} {} for 200 services, {} for 800",
                    small[index], large[index]
                ));
            }
        }
    }
    assert!(slow.is_empty(), "{slow:#?}");
}

/// The large sample, whose values written inline (HCL's lists and maps)
/// each cost a walk from the top.
#[test]
fn a_line_typed_far_from_a_host_range_walks_the_large_sample_a_few_times() {
    let mut slow = Vec::new();
    for language in Language::ALL.iter().copied() {
        let counts = typing_work(language, sample(language), "target");
        let walks = counts[kind(Work::PathLookup)];
        if walks > 4 {
            slow.push(format!("{language}: {walks} walks from the top"));
        }
    }
    assert!(slow.is_empty(), "{slow:#?}");
}
