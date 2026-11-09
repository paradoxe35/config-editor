//! The paths read at many places of a text in one pass (`paths_at`, what
//! host ranges are placed by while editing as another language) are the
//! ones `path_at` reads at each place alone, from the top, in every
//! language: at every place of documents in each language's forms, of
//! texts that do not parse, and at a few places of each line of the large
//! samples.

use super::super::Language;
use super::super::paths::{path_at, paths_at};
use super::fields_tests::{FORMS, SAMPLES};
use super::tests::{DOCUMENTS, report};

/// Texts mid-typing, which do not parse.
const BROKEN: [(Language, &str); 5] = [
    (
        Language::YAML,
        "web:\n  http: 80\n  hosts:\n    - a\n  flow: {k: [1, \nlast: \"x\n",
    ),
    (
        Language::JSON,
        "{\n  \"web\": {\n    \"http\": 80,\n    \"ho\n  \"last\": [1, {\"a\": \n}\n",
    ),
    (
        Language::TOML,
        "[web]\nhttp = 80\nflow = { k = [1, \n[[routes\npath = \"/a\nlast = '''x\n",
    ),
    (
        Language::HCL,
        "web {\n  http = 80\n  flow = { k = [1, \n}\nresource \"x\" {\n  name = \"${var.\n",
    ),
    (
        Language::XML,
        "<config>\n  <web a=\"1\n    <http>80</ht\n  <route><path>/a</path>\n  <!-- x\n</config>\n",
    ),
];

/// Where `paths_at` and `path_at` disagree on `text` at `places`.
fn disagreements(language: Language, text: &str, places: &[usize]) -> Vec<String> {
    let behavior = language.behavior();
    let at_once = paths_at(behavior, text, places);
    places
        .iter()
        .zip(at_once)
        .filter_map(|(place, read)| {
            let alone = path_at(behavior, text, *place);
            (read != alone)
                .then(|| format!("{language} at {place}: at once {read:?}, alone {alone:?}"))
        })
        .collect()
}

#[test]
fn paths_read_at_once_are_the_ones_read_alone_at_every_place_in_every_language() {
    let mut failures = Vec::new();
    for (language, text) in FORMS.into_iter().chain(DOCUMENTS).chain(BROKEN) {
        let places: Vec<usize> = (0..=text.encode_utf16().count() + 1).collect();
        failures.extend(disagreements(language, text, &places));
    }
    report(failures);
}

#[test]
fn paths_read_at_once_are_the_ones_read_alone_in_the_large_samples() {
    let mut failures = Vec::new();
    for (language, text) in SAMPLES {
        // Each line's start, its first text, and its middle.
        let mut places = Vec::new();
        let mut at = 0;
        for line in text.split('\n') {
            let width = line.encode_utf16().count();
            let indent = line.len() - line.trim_start().len();
            places.extend([at, at + indent, at + width / 2]);
            at += width + 1;
        }
        failures.extend(disagreements(language, text, &places));
    }
    report(failures);
}
