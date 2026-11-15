//! The check an Edit-as save of the whole document passes before it
//! reaches the host (read-only values, placeholders glued or moved off
//! their fields): its work grows with the document, never with the
//! document times its placeholders or locks. (After a large Replace All in
//! a view, the save's pause grew four times for each doubling.)

use crate::editor::Language;
use crate::editor::paths::FieldPath;
use crate::editor::secret_options::SecretOptions;
use crate::editor::test_state::TestEditor;
use crate::editor::work_count::{Work, counted_all};

/// `blocks` services in JSON, each with a placeholder and a port, the
/// first one's port read-only.
fn host(blocks: usize) -> String {
    let services: Vec<String> = (0..blocks)
        .map(|index| {
            format!(
                "    \"svc_{index}\": {{\n      \"key\": \"«secret {index:08x}»\",\n      \"image\": \"app-{index}\",\n      \"port\": {index}\n    }}"
            )
        })
        .collect();
    format!(
        "{{\n  \"services\": {{\n{}\n  }}\n}}\n",
        services.join(",\n")
    )
}

/// The work of checking a save of `blocks` services after every image was
/// replaced at once.
fn work(blocks: usize) -> [usize; 3] {
    let text = host(blocks);
    let (_owner, state) = TestEditor {
        read_only_paths: vec![FieldPath::from("/services/svc_0/port")],
        secrets: Some(SecretOptions::default().with_placeholder("«secret [0-9a-f]{8}»")),
        ..TestEditor::new(Language::JSON, &text)
    }
    .mount();
    let new = text.replace("\"app-", "\"img-");
    let (result, counts) = counted_all(|| state.check_host_save(&text, &new));
    assert!(result.is_ok(), "{blocks}: {result:?}");
    let count = |kind: Work| {
        counts[Work::ALL
            .iter()
            .position(|known| *known == kind)
            .unwrap_or_default()]
    };
    [
        count(Work::DocumentRead),
        count(Work::TokenScan),
        count(Work::SpanScan),
    ]
}

#[test]
fn a_whole_document_save_check_costs_work_linear_in_the_document() {
    let (small, large) = (work(100), work(200));
    for (kind, (small, large)) in ["document reads", "token scans", "span scans"]
        .iter()
        .zip(small.iter().zip(large.iter()))
    {
        assert!(
            *large <= 2 * *small + 4,
            "{kind}: {small} for 100 services, {large} for 200"
        );
    }
}

/// The large JSON sample written `copies` times, each under a key of its
/// own (its placeholders, 153 a copy, each its field's).
fn copies(copies: usize) -> String {
    let sample = include_str!("../../../../tests/fixtures/samples/tunnel.large.json").trim_end();
    let entries: Vec<String> = (0..copies)
        .map(|index| format!("  \"copy_{index}\": {sample}"))
        .collect();
    format!("{{\n{}\n}}\n", entries.join(",\n"))
}

/// Every kind of work the save check of an Edit-as (YAML) view of
/// `count` copies costs after a Replace All over the whole document.
fn view_work(count: usize) -> Vec<(Work, usize)> {
    let text = copies(count);
    let (_owner, state) = TestEditor {
        edit_as: Some(Language::YAML),
        read_only_paths: vec![FieldPath::from("/copy_0/listen")],
        secrets: Some(SecretOptions::default().with_placeholder("«secret [0-9a-f]+»")),
        ..TestEditor::new(Language::JSON, &text)
    }
    .mount();
    let new = text.replace("tunnel.example.com", "tunnel.example.org");
    assert_ne!(new, text, "the replace changes the text");
    let (result, counts) = counted_all(|| state.check_host_save(&text, &new));
    assert!(result.is_ok(), "{count}: {result:?}");
    Work::ALL
        .iter()
        .zip(counts)
        .map(|(kind, count)| (*kind, count))
        .collect()
}

/// After a Replace All in an Edit-as view of the large sample written two
/// and four times, the save check grows with the text: no kind of work
/// more than about doubles (its pause grew four times for each doubling).
#[test]
fn an_edit_as_save_check_after_a_replace_all_costs_work_linear_in_the_text() {
    let (small, large) = (view_work(2), view_work(4));
    let costly: Vec<String> = small
        .iter()
        .zip(&large)
        .filter(|((_, small), (_, large))| *large > 2 * *small + 20)
        .map(|((kind, small), (_, large))| {
            format!("{kind:?}: {small} for two copies, {large} for four")
        })
        .collect();
    assert!(costly.is_empty(), "{costly:#?}");
}
