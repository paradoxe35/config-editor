//! Conversion round trips on random documents in each language's rich
//! forms, for every pair: `A → B` and `A → B → A` give the same value
//! wherever the conversion lists no loss (or the conversion is refused
//! with a message), numbers keep their exact value (compared as written,
//! so a big integer drifting through a float shows), and an Edit-as view
//! opened and switched back with no edit leaves the host's text as it
//! was.

use crate::editor::language::pointer_key;
use leptos::prelude::*;
use serde_json::Value;

use super::docs::{LANGUAGES, literal};
use super::rich::{Node, random_tree, render, size, smaller_trees};
use super::runner::{Case, Failure, Rng, cases, run};
use crate::editor::languages::Language;
use crate::editor::sync::switch_to;
use crate::editor::test_state::TestEditor;

#[derive(Clone, Debug)]
pub(super) struct ConvertCase {
    from: Language,
    to: Language,
    tree: Node,
    extra: bool,
}

impl ConvertCase {
    fn text(&self) -> String {
        render(self.from, &self.tree, self.extra)
    }
}

impl Case for ConvertCase {
    fn code(&self) -> String {
        format!(
            "#[test]\nfn convert_repro() {{\n    super::convert::replay(Language::{}, Language::{}, {});\n}}",
            self.from.label(),
            self.to.label(),
            literal(&self.text())
        )
    }

    fn shrinks(&self) -> Vec<Self> {
        let mut out: Vec<Self> = smaller_trees(&self.tree)
            .into_iter()
            .map(|tree| Self {
                tree,
                ..self.clone()
            })
            .collect();
        if self.extra {
            out.insert(
                0,
                Self {
                    extra: false,
                    ..self.clone()
                },
            );
        }
        out
    }

    fn size(&self) -> usize {
        size(&self.tree) * 2 + usize::from(self.extra)
    }
}

fn generate(rng: &mut Rng) -> Option<ConvertCase> {
    let from = *rng.pick(&LANGUAGES);
    let to = *rng.pick(&LANGUAGES);
    if from == to {
        return None;
    }
    let tree = random_tree(rng, from, 2);
    let case = ConvertCase {
        from,
        to,
        tree,
        extra: rng.chance(15),
    };
    // A tree the language does not read (an alias before its anchor, a
    // TOML table written twice) is no case.
    from.parse(&case.text()).is_ok().then_some(case)
}

/// Pointers where `a` and `b` differ, as deep as they differ.
fn differences(a: &Value, b: &Value, at: String, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Object(left), Value::Object(right)) => {
            for (key, value) in left {
                let pointer = format!("{at}/{}", pointer_key(key));
                match right.get(key) {
                    Some(other) => differences(value, other, pointer, out),
                    None => out.push(pointer),
                }
            }
            for key in right.keys() {
                if !left.contains_key(key) {
                    out.push(format!("{at}/{}", pointer_key(key)));
                }
            }
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
            for (index, (value, other)) in left.iter().zip(right).enumerate() {
                differences(value, other, format!("{at}/{index}"), out);
            }
        }
        (Value::Number(left), Value::Number(right)) => {
            // The same number, however held (1.0 read as a float or 1).
            if left.as_f64() != right.as_f64() {
                out.push(at);
            }
        }
        _ if a == b => {}
        _ => out.push(at),
    }
}

fn covered(pointer: &str, losses: &[String]) -> bool {
    losses.iter().any(|loss| {
        loss == "/"
            || loss.is_empty()
            || pointer == loss
            || pointer.starts_with(&format!("{loss}/"))
            || loss.starts_with(&format!("{pointer}/"))
    })
}

/// A number as written, made canonical: sign, digits without leading or
/// trailing zeros, and the exponent of the last digit; hex, octal and
/// binary integers in decimal; `None` for no number.
fn canonical_number(written: &str) -> Option<String> {
    let text: String = written.trim().chars().filter(|c| *c != '_').collect();
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest.to_string()),
        None => (false, text.trim_start_matches('+').to_string()),
    };
    for (prefix, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
        if let Some(digits) = body.strip_prefix(prefix) {
            let value = u128::from_str_radix(digits, radix).ok()?;
            return canonical_number(&format!("{}{value}", if negative { "-" } else { "" }));
        }
    }
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(at) => (&body[..at], body[at + 1..].parse::<i64>().ok()?),
        None => (body.as_str(), 0),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty() && fraction.is_empty()
        || !whole
            .chars()
            .chain(fraction.chars())
            .all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let digits = format!("{whole}{fraction}");
    let mut exponent = exponent - fraction.len() as i64;
    let trimmed = digits.trim_start_matches('0');
    if trimmed.is_empty() {
        return Some("0".into());
    }
    let mut significant = trimmed.to_string();
    while significant.ends_with('0') {
        significant.pop();
        exponent += 1;
    }
    Some(format!(
        "{}{significant}e{exponent}",
        if negative { "-" } else { "" }
    ))
}

/// The number written for the field at `pointer` in `text`: the last
/// number-like run in its range.
fn written_number(language: Language, text: &str, pointer: &str) -> Option<String> {
    let range = language.range_of(text, pointer)?;
    let slice = range.slice(text);
    let slice = slice.as_str();
    // The value is last on its line (`key: 1`, `<k>1</k>`, `k = 1`).
    let value_part = slice
        .rsplit([':', '=', '>'])
        .find(|part| part.chars().any(|c| c.is_ascii_digit()))
        .unwrap_or(slice);
    let token: String = value_part
        .trim()
        .trim_end_matches(|c: char| !c.is_ascii_alphanumeric())
        .rsplit(|c: char| c.is_whitespace() || c == '<' || c == '"' || c == ',')
        .next()
        .unwrap_or_default()
        .to_string();
    canonical_number(&token)
}

/// Numbers whose written value changed from `source` to `target`, at
/// pointers no loss covers.
fn drifted(
    (from, source, value): (Language, &str, &Value),
    (to, target): (Language, &str),
    losses: &[String],
) -> Vec<String> {
    let mut numbers = Vec::new();
    numbers_in(value, String::new(), &mut numbers);
    numbers
        .into_iter()
        .filter(|pointer| !covered(pointer, losses))
        .filter_map(|pointer| {
            let before = written_number(from, source, &pointer)?;
            let after = written_number(to, target, &pointer)?;
            (before != after).then(|| {
                // Rounded to the same double: only the digits a double
                // cannot hold are gone.
                let double = |canonical: &str| canonical.replace('e', "E").parse::<f64>().ok();
                let rounded = double(&before).is_some() && double(&before) == double(&after);
                format!(
                    "{}{pointer}: {before} became {after}",
                    if rounded { "[double] " } else { "" }
                )
            })
        })
        .collect()
}

/// The class of a drift: digits past a double's rounded away, or a
/// number changed.
fn drift_kind(drift: &[String]) -> &'static str {
    if drift.iter().all(|found| found.starts_with("[double]")) {
        "a decimal with more digits than a double holds is rounded"
    } else {
        "a number is written with another value"
    }
}

fn numbers_in(value: &Value, at: String, out: &mut Vec<String>) {
    match value {
        Value::Number(_) => out.push(at),
        Value::Object(map) => {
            for (key, value) in map {
                numbers_in(value, format!("{at}/{}", pointer_key(key)), out);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                numbers_in(item, format!("{at}/{index}"), out);
            }
        }
        _ => {}
    }
}

/// Checks `source` (in `from`) converted to `to` and back.
pub(super) fn check_text(from: Language, to: Language, source: &str) -> Result<(), Failure> {
    let pair = format!("{from}→{to}");
    let Ok(value) = from.parse(source) else {
        return Ok(());
    };
    let conversion = match from.convert(source, to) {
        Ok(conversion) => conversion,
        Err(error) if error.to_string().trim().is_empty() => {
            return Err(Failure::new(
                format!("refused without a message ({pair})"),
                format!("{error:?}"),
            ));
        }
        Err(_) => return Ok(()),
    };
    let there = match to.parse(&conversion.text) {
        Ok(there) => there,
        Err(error) => {
            return Err(Failure::new(
                format!("converted text does not read back ({pair})"),
                format!("{error}\nconverted:\n{}", conversion.text),
            ));
        }
    };
    let mut changed = Vec::new();
    differences(&value, &there, String::new(), &mut changed);
    changed.retain(|pointer| !covered(pointer, &conversion.losses));
    if !changed.is_empty() {
        return Err(Failure::new(
            format!("value changed with no loss listed ({pair})"),
            format!(
                "at {changed:?}, losses {:?}\nconverted:\n{}\nread as {there}\nsource read as {value}",
                conversion.losses, conversion.text
            ),
        ));
    }
    let drift = drifted(
        (from, source, &value),
        (to, &conversion.text),
        &conversion.losses,
    );
    if !drift.is_empty() {
        return Err(Failure::new(
            format!("{} with no loss listed ({pair})", drift_kind(&drift)),
            format!("{drift:?}\nconverted:\n{}", conversion.text),
        ));
    }
    let back = match to.convert(&conversion.text, from) {
        Ok(back) => back,
        Err(_) => return Ok(()),
    };
    let mut losses = conversion.losses.clone();
    losses.extend(back.losses.iter().cloned());
    let again = match from.parse(&back.text) {
        Ok(again) => again,
        Err(error) => {
            return Err(Failure::new(
                format!("converted back, the text does not read ({pair}→{from})"),
                format!("{error}\nback:\n{}", back.text),
            ));
        }
    };
    let mut changed = Vec::new();
    differences(&value, &again, String::new(), &mut changed);
    changed.retain(|pointer| !covered(pointer, &losses));
    if !changed.is_empty() {
        return Err(Failure::new(
            format!("round trip changed a value with no loss listed ({pair}→{from})"),
            format!(
                "at {changed:?}, losses {losses:?}\nthere:\n{}\nback:\n{}",
                conversion.text, back.text
            ),
        ));
    }
    let drift = drifted((from, source, &value), (from, &back.text), &losses);
    if !drift.is_empty() {
        return Err(Failure::new(
            format!("round trip: {} ({pair}→{from})", drift_kind(&drift)),
            format!("{drift:?}\nback:\n{}", back.text),
        ));
    }
    Ok(())
}

/// Opens `source` in an Edit-as view of `to` and switches back with no
/// edit: the host's text and the text shown are the host's own.
pub(super) fn check_view(from: Language, to: Language, source: &str) -> Result<(), Failure> {
    let pair = format!("{from} host as {to}");
    let (_owner, state) = TestEditor::new(from, source).mount();
    switch_to(state, to);
    if state.sync().edit_language.get_untracked() != to {
        // Refused (a document the view cannot show), with its message.
        return Ok(());
    }
    super::actions::pause(state);
    let host = state.sync().host_text.get_value();
    if host != source {
        return Err(Failure::new(
            format!("a view with no edit changed the host's text ({pair})"),
            format!(
                "host now:\n{host}\nview:\n{}",
                state.doc().text.get_untracked()
            ),
        ));
    }
    switch_to(state, from);
    let shown = state.doc().text.get_untracked();
    if shown != source {
        return Err(Failure::new(
            format!("switched back with no edit, the text differs ({pair})"),
            format!("shown:\n{shown}"),
        ));
    }
    Ok(())
}

fn check(case: &ConvertCase) -> Result<(), Failure> {
    let text = case.text();
    check_text(case.from, case.to, &text)?;
    check_view(case.from, case.to, &text)
}

/// Runs one case, failing with what it found (for a pasted repro).
#[allow(dead_code, reason = "called by pasted repros")]
pub(super) fn replay(from: Language, to: Language, source: &str) {
    if let Err(failure) = check_text(from, to, source).and_then(|()| check_view(from, to, source)) {
        panic!("{}\n{}", failure.class, failure.detail);
    }
}

#[test]
fn conversions_keep_values_and_numbers_or_list_the_loss() {
    run("convert", cases(800), &generate, &check);
}

#[test]
fn canonical_numbers_compare_values_as_written() {
    assert_eq!(canonical_number("1.50"), canonical_number("1.5"));
    assert_eq!(canonical_number("0x1F"), canonical_number("31"));
    assert_eq!(canonical_number("1e3"), canonical_number("1000"));
    assert_eq!(canonical_number("1_000"), canonical_number("1000.0"));
    assert_ne!(
        canonical_number("9007199254740993"),
        canonical_number("9007199254740992")
    );
    assert_eq!(canonical_number("-0"), Some("0".into()));
    assert_eq!(canonical_number("abc"), None);
}
