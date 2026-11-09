//! The field a host range starts on, looked up where the fields are written
//! (`HostFields`), is the one reading the text from the top (`path_at`)
//! finds, at every place of documents written in each language's forms;
//! and the spans read at once for many fields are the ones read alone.

use super::super::Language;
use super::super::paths::{
    TextPosition, fields, memo_key_span, memo_value_span, path_at, with_spans,
};
use super::fields::HostFields;
use super::tests::report;

/// Documents in each language's forms: nested blocks, lists of scalars and
/// of maps, inline (flow) containers, quoted and dotted keys, attributes,
/// labels, comments.
pub(super) const FORMS: [(Language, &str); 5] = [
    (
        Language::YAML,
        "listen: 7835 # port\nweb:\n  http: 80\n  \"quoted key\": 'q'\n  hosts:\n    - a.example\n    - b.example\n  routes:\n    - path: /a\n      to: x\n    - {path: /b, to: y}\n  flow: {k: 1, l: [2, 3]}\n  empty: {}\n  text: |\n    line one\n    line two\nlast: true\n",
    ),
    (
        Language::JSON,
        "{\n  \"listen\": 7835,\n  \"web\": {\n    \"http\": 80,\n    \"hosts\": [\"a.example\", \"b.example\"],\n    \"routes\": [{\"path\": \"/a\", \"to\": \"x\"}, {\"path\": \"/b\"}],\n    \"empty\": {},\n    \"deep\": {\"k\": {\"l\": [1, 2]}}\n  },\n  \"last\": true\n}\n",
    ),
    (
        Language::TOML,
        "listen = 7835 # port\nsite.name = \"n\"\n\n[web]\nhttp = 80\n\"quoted key\" = 'q'\nhosts = [\"a.example\", \"b.example\"]\nflow = { k = 1, l = [2, 3], m = { n = 4 } }\n\n[[routes]]\npath = \"/a\"\nto = \"x\"\n\n[[routes]]\npath = \"/b\"\n\n[web.deep]\nk = 1\n",
    ),
    (
        Language::HCL,
        "listen = 7835 # port\nweb {\n  http = 80\n  hosts = [\"a.example\", \"b.example\"]\n  flow = { k = 1, l = [2, 3] }\n}\nresource \"aws_x\" \"main\" {\n  size = 2\n  name = \"${var.n}-$${x}\"\n  tags = {\n    env = \"dev\"\n  }\n}\nroute {\n  path = \"/a\"\n}\nroute {\n  path = \"/b\"\n}\n",
    ),
    (
        Language::XML,
        "<config version=\"2\">\n  <listen>7835</listen>\n  <!-- web -->\n  <web secure=\"true\">\n    <http>80</http>\n    <hosts>a.example</hosts>\n    <hosts>b.example</hosts>\n    <route><path>/a</path><to>x</to></route>\n    <empty/>\n  </web>\n  <last>true</last>\n</config>\n",
    ),
];

/// The large samples of every language.
pub(super) const SAMPLES: [(Language, &str); 5] = [
    (
        Language::YAML,
        include_str!("../../../tests/fixtures/samples/tunnel.large.yaml"),
    ),
    (
        Language::JSON,
        include_str!("../../../tests/fixtures/samples/tunnel.large.json"),
    ),
    (
        Language::TOML,
        include_str!("../../../tests/fixtures/samples/tunnel.large.toml"),
    ),
    (
        Language::HCL,
        include_str!("../../../tests/fixtures/samples/tunnel.large.hcl"),
    ),
    (
        Language::XML,
        include_str!("../../../tests/fixtures/samples/tunnel.large.xml"),
    ),
];

/// Where the two lookups disagree on `text` (in `language`), at each of
/// `columns(line)` of every line.
fn disagreements(
    language: Language,
    text: &str,
    columns: &dyn Fn(&str) -> Vec<usize>,
) -> Vec<String> {
    let host = language.behavior();
    let document = host.parse(text).expect("the document parses");
    let found = fields(&document);
    let pointers: Vec<String> = found.iter().map(|(pointer, _)| pointer.clone()).collect();
    let mut failures = Vec::new();
    with_spans(host, text, &pointers, || {
        let written = HostFields::new(host, text, found);
        for (index, line) in text.split('\n').enumerate() {
            for column in columns(line) {
                let position = TextPosition::new(index + 1, column);
                let Some(fast) = written.at(position) else {
                    continue;
                };
                let read = path_at(host, text, position.to_offset(text))
                    .filter(|path| !path.is_root())
                    .map(|path| path.to_pointer());
                if read.as_deref() != Some(fast) {
                    failures.push(format!(
                        "{language} {}:{column} ({line:?}): looked up {fast}, read {read:?}",
                        index + 1
                    ));
                }
            }
        }
    });
    failures
}

#[test]
fn the_field_looked_up_is_the_one_read_at_every_place_in_every_language() {
    let mut failures = Vec::new();
    let every_column = |line: &str| (1..=line.chars().count() + 1).collect::<Vec<_>>();
    for (language, text) in FORMS.into_iter().chain(super::tests::DOCUMENTS) {
        failures.extend(disagreements(language, text, &every_column));
    }
    report(failures);
}

/// The large samples, at a few places of each line (reading each from the
/// top is slow).
#[test]
fn the_field_looked_up_is_the_one_read_in_the_large_samples() {
    // The first text, its middle and its last character.
    let some_columns = |line: &str| {
        let width = line.chars().count();
        let first = line.chars().take_while(|c| c.is_whitespace()).count() + 1;
        vec![first, (first + width) / 2, width.max(1)]
    };
    let mut failures = Vec::new();
    for (language, text) in SAMPLES {
        failures.extend(disagreements(language, text, &some_columns));
    }
    report(failures);
}

/// Every field's key and value span, each spelling (`/0` steps left out)
/// and paths written nowhere, read at once in one batch and alone outside
/// one, agree in every language.
#[test]
fn spans_read_at_once_are_the_ones_read_alone_in_every_language() {
    let mut failures = Vec::new();
    for (language, text) in FORMS
        .into_iter()
        .chain(super::tests::DOCUMENTS)
        .chain(SAMPLES)
    {
        let behavior = language.behavior();
        let document = behavior.parse(text).expect("the document parses");
        // Each read alone reads the text: some of a long one's fields.
        let every = if text.len() > 10_000 { 9 } else { 1 };
        let mut pointers: Vec<String> = fields(&document)
            .into_iter()
            .step_by(every)
            .flat_map(|(pointer, _)| {
                let lone = pointer.replace("/0/", "/");
                let lone = lone.strip_suffix("/0").unwrap_or(&lone).to_string();
                [format!("{pointer}/missing"), lone, pointer]
            })
            .collect();
        pointers.extend(["/missing".to_string(), "/0".to_string(), String::new()]);
        let alone: Vec<_> = pointers
            .iter()
            .map(|pointer| {
                (
                    behavior.key_span(text, pointer),
                    behavior.value_span(text, pointer),
                )
            })
            .collect();
        let batched: Vec<_> = with_spans(behavior, text, &pointers, || {
            pointers
                .iter()
                .map(|pointer| {
                    (
                        memo_key_span(behavior, text, pointer),
                        memo_value_span(behavior, text, pointer),
                    )
                })
                .collect()
        });
        for ((pointer, alone), batched) in pointers.iter().zip(alone).zip(batched) {
            if alone != batched {
                failures.push(format!(
                    "{language} {pointer:?}: alone {alone:?}, at once {batched:?}"
                ));
            }
        }
    }
    report(failures);
}
