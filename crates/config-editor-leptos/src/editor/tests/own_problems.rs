//! The editor's own problems as `on_problems` reports them: syntax, schema
//! and secret problems, never the host's, following the text checked after
//! each pause, and changing only when they change.

use std::sync::Arc;

use leptos::prelude::*;

use crate::editor::diagnostics::{DiagnosticOrigin, Severity, build_schema};
use crate::editor::host_diagnostics::HostDiagnostic;
use crate::editor::languages::Language;
use crate::editor::test_state::TestEditor;

const SCHEMA: &str = r#"{"type": "object", "properties": {"listen": {"type": "integer"}}}"#;

#[test]
fn own_problems_follow_the_checked_text_and_leave_out_the_hosts() {
    let mut editor = TestEditor::new(Language::YAML, "listen: 1\n");
    editor.schema = Some(Arc::new(build_schema(SCHEMA).expect("the schema")));
    let host = RwSignal::new(vec![HostDiagnostic::at_path(
        "/listen",
        Severity::Error,
        "taken on the server",
    )]);
    editor.host_diagnostics = host.into();
    let (_owner, state) = editor.mount();
    let own = state.problems().own;
    // How many times what it holds changed (a memo reruns only then).
    let runs = StoredValue::new(0);
    let heard = Memo::new(move |_| {
        runs.update_value(|runs| *runs += 1);
        own.get()
    });
    let check = |text: &str| {
        state.doc().text.set(text.to_string());
        state.problems().diagnosed_text.set(text.to_string());
        heard.get_untracked()
    };
    let messages = |problems: &[crate::editor::diagnostics::Diagnostic]| -> Vec<String> {
        problems
            .iter()
            .map(|problem| problem.message.clone())
            .collect()
    };

    let valid = check("listen: 1\n");
    assert_eq!(valid, Vec::new(), "the host's problem is not the editor's");
    // ...though the editor shows it.
    assert!(
        state
            .problems()
            .diagnostics
            .get_untracked()
            .iter()
            .any(|problem| problem.origin == DiagnosticOrigin::Host)
    );
    assert_eq!(runs.get_value(), 1);

    let schema = check("listen: one\n");
    assert_eq!(schema.len(), 1, "{:?}", messages(&schema));
    assert!(schema[0].message.starts_with("/listen"), "{schema:?}");
    assert_eq!(schema[0].origin, DiagnosticOrigin::Editor);

    let syntax = check("listen: [1\n");
    assert_eq!(syntax.len(), 1, "{:?}", messages(&syntax));
    assert_eq!(syntax[0].severity, Severity::Error);
    assert_eq!(syntax[0].origin, DiagnosticOrigin::Editor);
    assert_ne!(syntax[0].message, schema[0].message);
    assert_eq!(runs.get_value(), 3);

    // Fixed: none again.
    assert_eq!(check("listen: 2\n"), Vec::new());
    assert_eq!(runs.get_value(), 4);
    // Another valid text, a new host list: nothing to report.
    check("listen: 3\n");
    host.set(vec![HostDiagnostic::document(Severity::Warning, "stale")]);
    assert_eq!(heard.get_untracked(), Vec::new());
    assert_eq!(runs.get_value(), 4, "no change, no report");
}
