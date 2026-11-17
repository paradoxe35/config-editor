//! A document typed in an Edit-as view with XML on one side (an XML view
//! of another host, or another view of an XML host) saves as the view
//! reads it (P2): the host reads back the values the view showed, typed
//! as the schema decides, or a documented XML spelling explains each value
//! it reads otherwise (see `xml_spelling`); and the host has a problem
//! exactly when the view shows one.

use serde_json::{Value, json};

use super::documents::{Shape, document};
use super::random::Rng;
use super::reference::{compile, messages, same, xml_cannot_hold};
use super::runner::{Case, Property, Verdict, failure, language, run};
use super::schemas::{Draw, schema};
use super::xml_spelling::{self, Sides};
use crate::editor::Language;
use crate::editor::bridge::Bridge;

pub(super) fn draw(rng: &mut Rng, index: usize) -> Case {
    let draw = Draw {
        marks: false,
        docs: false,
        depth: 2 + rng.below(2),
    };
    let schema = schema(rng, draw);
    let shape = Shape {
        wrong: 5 + rng.below(15),
        secrets: false,
        every_key: false,
    };
    let doc = document(rng, &schema, shape);
    let other = document(rng, &schema, shape);
    Case {
        language: language(index),
        pick: rng.below(4),
        schema,
        doc,
        other,
    }
}

/// The view's language: XML for another host, else the one `pick` names.
fn edit_language(case: &Case) -> Language {
    if case.language != Language::XML {
        return Language::XML;
    }
    let others: Vec<Language> = Language::ALL
        .iter()
        .copied()
        .filter(|language| *language != Language::XML)
        .collect();
    others[case.pick % others.len()]
}

pub(super) fn check(case: &Case) -> Verdict {
    let Some(both) = compile(&case.schema) else {
        return Verdict::Skipped;
    };
    let host = case.language;
    let edit = edit_language(case);
    let Some(host_text) = both.written(host, &case.doc) else {
        return Verdict::Skipped;
    };
    let bridge = Bridge::new(host, edit, Some(both.editor.bundle.clone())).unwrap();
    // The view shows the host's problems too while it cannot show the
    // host exactly.
    let inexact = match bridge.to_edit(&host_text) {
        Ok(shown) => !shown.losses.is_empty(),
        Err(_) => return Verdict::Skipped,
    };
    // What XML cannot write is no value to type in an XML view.
    // Nor are a host's values the XML view cannot show (kept as they are
    // by its saves) part of what it reads.
    if edit == Language::XML && (xml_cannot_hold(&case.other) || xml_cannot_hold(&case.doc)) {
        return Verdict::Skipped;
    }
    let Some(typed) = both.written(edit, &case.other) else {
        return Verdict::Skipped;
    };
    let Some(view_value) = both.read(edit, &typed) else {
        return Verdict::Skipped;
    };
    let saved = match bridge.to_host(&typed, &host_text) {
        Ok(saved) => saved,
        // The save waits, telling why: no silent change.
        Err(_) => return Verdict::Skipped,
    };
    // A view typed as it was shown is unchanged: it saves nothing (its
    // host keeps every value as it was, read as the view may read it).
    if bridge
        .to_edit(&host_text)
        .is_ok_and(|shown| shown.text == typed)
    {
        return if saved == host_text {
            Verdict::Holds
        } else {
            Verdict::Fails(failure(
                "an unchanged view changes its host",
                format!("{host} host:\n{host_text}saved:\n{saved}"),
            ))
        };
    }
    let host_value = both.read(host, &saved);
    // A value the view leaves as the host has it keeps its host form (the
    // save writes nothing for it), whatever the view reads it as.
    let view_value = match &host_value {
        Some(host_value) => {
            // XML writes some values of two types alike (`"8"` and `8`, a
            // list of one and its item): typed alike, they are untouched.
            let alike = |typed: &Value, before: &Value| {
                typed == before
                    || (edit == Language::XML && {
                        let written = |value: &Value| both.written(edit, &json!({ "a": value }));
                        written(typed).is_some_and(|text| Some(text) == written(before))
                    })
            };
            untouched_from(&view_value, (&case.other, &case.doc), host_value, &alike)
        }
        None => view_value,
    };
    let seen = |what: &str| {
        format!(
            "{host} host:\n{host_text}{edit} view typed:\n{typed}reads {view_value}\nsaved:\n{saved}reads {host_value:?}\n{what}"
        )
    };
    if !host_value
        .as_ref()
        .is_some_and(|host| same(host, &view_value))
    {
        let before = both.read(host, &host_text).unwrap_or_default();
        let spelled = host_value.as_ref().and_then(|saved| {
            xml_spelling::explained(&Sides {
                both: &both,
                host,
                edit,
                before: &before,
                view: &view_value,
                saved,
            })
        });
        let class = if host == Language::XML && xml_cannot_hold(&view_value) {
            "a save writes what XML cannot hold, without waiting"
        } else {
            spelled.unwrap_or("a save reads otherwise than the view")
        };
        return Verdict::Fails(failure(class, seen("")));
    }
    let view_problems = both.view_problems((edit, &typed), (host, &saved), inexact);
    let host_problems = both.problems(host, &saved);
    if view_problems.is_empty() != host_problems.is_empty() {
        return Verdict::Fails(failure(
            "the view and its saved host disagree on problems",
            seen(&format!(
                "view problems:\n{}\nhost problems:\n{}",
                messages(&view_problems),
                messages(&host_problems)
            )),
        ));
    }
    Verdict::Holds
}

#[test]
fn values_typed_in_an_edit_as_view_save_as_the_view_reads_them() {
    run(&Property {
        name: "edit_as",
        check_path: "super::edit_as::check",
        cases: 250,
        shrink_budget: 1500,
        draw,
        check,
    });
}

/// `view` with each value `typed` (the document the view was typed from)
/// leaves as `before` (the host's document) has it, as `alike` tells,
/// taken from `host` (what the save reads back): untouched values keep
/// their host form.
fn untouched_from(
    view: &Value,
    (typed, before): (&Value, &Value),
    host: &Value,
    alike: &dyn Fn(&Value, &Value) -> bool,
) -> Value {
    if alike(typed, before) {
        return host.clone();
    }
    match (view, typed, before, host) {
        (Value::Object(view), Value::Object(typed), Value::Object(before), Value::Object(host)) => {
            Value::Object(
                view.iter()
                    .map(|(key, value)| {
                        let kept = match (typed.get(key), before.get(key), host.get(key)) {
                            (Some(typed), Some(before), Some(host)) => {
                                untouched_from(value, (typed, before), host, alike)
                            }
                            _ => value.clone(),
                        };
                        (key.clone(), kept)
                    })
                    .collect(),
            )
        }
        (Value::Array(view), Value::Array(typed), Value::Array(before), Value::Array(host))
            if [typed.len(), before.len(), host.len()] == [view.len(); 3] =>
        {
            Value::Array(
                view.iter()
                    .zip(typed.iter().zip(before).zip(host))
                    .map(|(value, ((typed, before), host))| {
                        untouched_from(value, (typed, before), host, alike)
                    })
                    .collect(),
            )
        }
        _ => view.clone(),
    }
}
