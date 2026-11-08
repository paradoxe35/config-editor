//! QA 21 (U21-6): a keystroke in a long JSON or XML document read the
//! whole document for the span (and path) of every key and value to find
//! the caret's place, twice (its suggestions' context, the caret's path):
//! a long task on every key. The caret's place reads no spans now.

use super::super::paths::{TextPosition, path_at};
use super::super::work_count::{Work, counted};
use super::Language;

#[test]
fn the_caret_context_and_path_read_no_spans_in_json_and_xml() {
    let items = 200;
    let json = format!(
        "{{\n{}\n}}\n",
        (0..items)
            .map(|i| format!("  \"svc_{i}\": {{\n    \"host\": \"h{i}\",\n    \"port\": {i}\n  }}"))
            .collect::<Vec<_>>()
            .join(",\n")
    );
    let xml = format!(
        "<root>\n{}</root>\n",
        (0..items)
            .map(|i| format!(
                "  <svc_{i}>\n    <host>h{i}</host>\n    <port>{i}</port>\n  </svc_{i}>\n"
            ))
            .collect::<String>()
    );
    let mut failures = Vec::new();
    for (language, text) in [(Language::JSON, json), (Language::XML, xml)] {
        let behavior = language.behavior();
        // At the end of a line in the middle, as typing there.
        let line = text.split('\n').count() / 2;
        let position = TextPosition::new(
            line,
            text.split('\n').nth(line - 1).unwrap().chars().count() + 1,
        );
        let cursor = position.to_offset(&text);
        let ((), scans) = counted(Work::SpanScan, || {
            let _ = behavior.completion_context(&text, cursor);
            let _ = path_at(behavior, &text, cursor);
            let _ = behavior.entry_started(&text, cursor);
        });
        if scans > 0 {
            failures.push(format!(
                "{language}: {scans} scans of every span for the caret's place"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
