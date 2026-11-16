//! Cases from hands-on use of the demo, checked against its tunnel schema,
//! by feature.

mod comments;
mod completion_keys;
mod completion_values;
mod enter;
mod hover;
mod lists;
mod reading;
mod renames;
mod schema_errors;
mod suggestion_triggers;
mod syntax_errors;
mod table_headers;
mod typing;

use crate::editor::completion::{accept_marked, completions};
use crate::editor::languages::Language;
use crate::editor::schema::{SchemaBundle, build_schema};

pub(super) fn burrow() -> SchemaBundle {
    build_schema(include_str!(
        "../../../../tests/fixtures/tunnel.schema.json"
    ))
    .unwrap()
}

pub(super) fn accept(language: Language, marked: &str, label: &str) -> String {
    accept_marked(marked, label, &burrow(), language.behavior())
}

pub(super) fn labels(language: Language, marked: &str) -> Vec<String> {
    let at = marked.find('|').unwrap();
    let source = marked.replacen('|', "", 1);
    completions(
        &source,
        source[..at].encode_utf16().count(),
        true,
        Some(&burrow()),
        true,
        language.behavior(),
    )
    .into_iter()
    .map(|option| option.label)
    .collect()
}

#[track_caller]
pub(super) fn assert_parses(language: Language, text: &str) {
    if let Err(error) = language.parse(&text.replace('|', "")) {
        panic!("{language} does not parse: {error}\n{text}");
    }
}
