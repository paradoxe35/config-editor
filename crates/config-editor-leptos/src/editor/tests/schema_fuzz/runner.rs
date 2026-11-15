//! Running a property over its random cases: each failure shrunk (the
//! first of its class), grouped by class, printed as a test to paste, and
//! allowed only when a known finding names its class (see [`known`]).

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use serde_json::Value;

use super::known;
use super::random::{Rng, cases, seed};
use super::shrink::shrink;
use crate::editor::Language;

/// One case: a schema, a document, a second value some properties use (the
/// document an Edit-as view types, …), the language it is written in and a
/// number choosing among the property's places (a caret, a second
/// language).
#[derive(Clone, Debug)]
pub(super) struct Case {
    pub(super) language: Language,
    pub(super) pick: usize,
    pub(super) schema: Value,
    pub(super) doc: Value,
    pub(super) other: Value,
}

/// A failed check: its class (what went wrong, without the case's
/// particulars) and what was seen.
#[derive(Clone, Debug)]
pub(super) struct Failure {
    pub(super) class: String,
    pub(super) detail: String,
}

pub(super) fn failure(class: impl Into<String>, detail: impl Into<String>) -> Failure {
    Failure {
        class: class.into(),
        detail: detail.into(),
    }
}

/// What a check made of a case.
pub(super) enum Verdict {
    /// Checked and fine.
    Holds,
    /// Not checkable (the schema does not compile, the language cannot
    /// write the document).
    Skipped,
    Fails(Failure),
}

/// A property: its name, the path of its check (for the printed tests),
/// how it draws a case, and its check.
pub(super) struct Property {
    pub(super) name: &'static str,
    pub(super) check_path: &'static str,
    pub(super) cases: usize,
    /// Most checks shrinking one failure makes.
    pub(super) shrink_budget: usize,
    pub(super) draw: fn(&mut Rng, usize) -> Case,
    pub(super) check: fn(&Case) -> Verdict,
}

/// A check that panicked is a failure of its own class.
pub(super) fn verdict(check: fn(&Case) -> Verdict, case: &Case) -> Verdict {
    match catch_unwind(AssertUnwindSafe(|| check(case))) {
        Ok(verdict) => verdict,
        Err(panic) => {
            let message = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|text| (*text).to_string()))
                .unwrap_or_default();
            let first: String = message
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(80)
                .collect();
            Verdict::Fails(failure(format!("panic: {first}"), message))
        }
    }
}

struct Class {
    count: usize,
    languages: Vec<Language>,
    shrunk: Case,
    detail: String,
    excused: Option<&'static str>,
}

/// Runs `property` over its cases and fails on any class no known finding
/// allows, printing each class's smallest case.
pub(super) fn run(property: &Property) {
    let total = cases(property.cases);
    let mut classes: BTreeMap<String, Class> = BTreeMap::new();
    let (mut held, mut skipped) = (0, 0);
    let only: Option<usize> = std::env::var("SCHEMA_FUZZ_ONLY")
        .ok()
        .and_then(|index| index.parse().ok());
    for index in 0..total {
        if only.is_some_and(|only| only != index) {
            continue;
        }
        let mut rng = Rng::new(seed(property.name, index));
        let case = (property.draw)(&mut rng, index);
        if std::env::var_os("SCHEMA_FUZZ_TRACE").is_some() {
            eprintln!(
                "{} case {index}: {:?} pick {} schema {} doc {} other {}",
                property.name, case.language, case.pick, case.schema, case.doc, case.other
            );
        }
        match verdict(property.check, &case) {
            Verdict::Holds => held += 1,
            Verdict::Skipped => skipped += 1,
            Verdict::Fails(found) => {
                // A class is told apart by the finding allowing it: a case
                // the first one's finding does not explain is shrunk too.
                let key = format!(
                    "{}{}",
                    found.class,
                    known::excused(property.name, &found.class, &case)
                        .map(|finding| format!(" [{finding}]"))
                        .unwrap_or_default()
                );
                if let Some(class) = classes.get_mut(&key) {
                    class.count += 1;
                    if !class.languages.contains(&case.language) {
                        class.languages.push(case.language);
                    }
                    continue;
                }
                let check = property.check;
                let shrunk = shrink(
                    case.clone(),
                    &found.class,
                    property.shrink_budget,
                    &|case| verdict(check, case),
                );
                let detail = match verdict(check, &shrunk) {
                    Verdict::Fails(failure) => failure.detail,
                    _ => found.detail.clone(),
                };
                let excused = known::excused(property.name, &found.class, &shrunk);
                classes.insert(
                    key,
                    Class {
                        count: 1,
                        languages: vec![case.language],
                        shrunk,
                        detail,
                        excused,
                    },
                );
            }
        }
    }
    let mut report = format!(
        "{}: {total} cases, {held} held, {skipped} skipped, {} classes of failure\n",
        property.name,
        classes.len()
    );
    for (name, class) in &classes {
        report.push_str(&format!(
            "\n== class: {name}\n   {} cases, languages {:?}{}\n   seen: {}\n{}\n",
            class.count,
            class.languages,
            class
                .excused
                .map(|finding| format!(", allowed: {finding}"))
                .unwrap_or_default(),
            class.detail.replace('\n', "\n   "),
            repro(property, &class.shrunk)
        ));
    }
    eprintln!("{report}");
    let open: Vec<&String> = classes
        .iter()
        .filter(|(_, class)| class.excused.is_none())
        .map(|(name, _)| name)
        .collect();
    assert!(
        open.is_empty(),
        "{}: classes no known finding allows: {open:?}\n{report}",
        property.name
    );
}

/// The shrunk case as a test to paste into this module.
fn repro(property: &Property, case: &Case) -> String {
    format!(
        "#[test]\nfn {}_repro() {{\n    let case = Case {{\n        language: Language::{},\n        pick: {},\n        schema: json!({}),\n        doc: json!({}),\n        other: json!({}),\n    }};\n    assert!(matches!({}(&case), Verdict::Holds));\n}}",
        property.name,
        language_constant(case.language),
        case.pick,
        case.schema,
        case.doc,
        case.other,
        property.check_path,
    )
}

fn language_constant(language: Language) -> &'static str {
    match language.label() {
        "YAML" => "YAML",
        "JSON" => "JSON",
        "TOML" => "TOML",
        "HCL" => "HCL",
        _ => "XML",
    }
}

/// The five built-in languages.
pub(super) fn language(index: usize) -> Language {
    Language::ALL[index % Language::ALL.len()]
}
