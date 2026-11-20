//! Other values a schema may allow than those a fill tried first: what a
//! list item distinct from the others, or a value a refused placeholder
//! leaves to find, is picked from.

use super::*;

/// Other values `schema` may allow than those `filled`: its enum members
/// and consts, its examples, up to `more` values after each filled one
/// and after its lower bound (the next numbers, the other boolean, other
/// text), the first `more` multiples its `multipleOf` allows past its
/// lower bound, and for text of a least length or a pattern, `more` texts
/// of that length (and a little longer) in small letters, capitals and
/// digits.
pub(super) fn others(schema: &Value, root: &Value, filled: &[Value], more: usize) -> Vec<Value> {
    let branches = resolve_branches(schema, root);
    let mut found: Vec<Value> = Vec::new();
    for branch in &branches {
        for keyword in ["enum", "examples"] {
            if let Some(Value::Array(values)) = branch.get(keyword) {
                found.extend(values.iter().cloned());
            }
        }
        if let Some(value) = branch.get("const") {
            found.push(value.clone());
        }
        if let Some(step) = branch.get("multipleOf").and_then(Value::as_f64) {
            found.extend(multiples(branch, step, more));
        }
        if let Some(least) = branch.get("minimum").and_then(Value::as_i64) {
            found.extend(after(&Value::from(least), more + 1));
        }
        if let Some(above) = branch.get("exclusiveMinimum").and_then(Value::as_i64) {
            found.extend(after(&Value::from(above), more + 1).into_iter().skip(1));
        }
        let least = branch.get("minLength").and_then(Value::as_u64);
        if least.is_some() || branch.get("pattern").is_some() {
            let least = least.map_or(1, |least| usize::try_from(least).unwrap_or(usize::MAX));
            found.extend(texts(least.max(1), more));
        }
    }
    for value in filled {
        found.extend(after(value, more));
    }
    found
}

/// The first `more` multiples of `step` from `branch`'s lower bound on
/// (from 0 without one), whole numbers where they are whole.
fn multiples(branch: &Value, step: f64, more: usize) -> Vec<Value> {
    if !(step.is_finite() && step > 0.0) {
        return Vec::new();
    }
    let bound = |keyword: &str| branch.get(keyword).and_then(Value::as_f64);
    let first = match (bound("minimum"), bound("exclusiveMinimum")) {
        (Some(least), _) => (least / step).ceil(),
        (None, Some(above)) => (above / step).floor() + 1.0,
        (None, None) => 0.0,
    };
    (0..=more)
        .map(|index| (first + index as f64) * step)
        .filter(|number| number.is_finite())
        .map(|number| {
            if number.fract() == 0.0 && number.abs() < 9.0e15 {
                Value::from(number as i64)
            } else {
                Value::from(number)
            }
        })
        .collect()
}

/// Up to `more` texts of each length from `least` to two more, in small
/// letters (`aa`, `ab`, …), capitals and digits; none for a least length
/// too long to write out.
fn texts(least: usize, more: usize) -> Vec<Value> {
    if least > MAX_TEXT_LENGTH {
        return Vec::new();
    }
    let mut found = Vec::new();
    for length in least..=least + 2 {
        for (first, base) in [(b'a', 26), (b'A', 26), (b'0', 10)] {
            found.extend((0..more).map(|index| Value::String(spelled(index, length, first, base))));
        }
    }
    found
}

/// Longest least length [`texts`] writes text of.
const MAX_TEXT_LENGTH: usize = 64;

/// `index` written in `length` symbols from `first` on, `base` of them
/// (the first symbol padding it on the left).
fn spelled(index: usize, length: usize, first: u8, base: usize) -> String {
    let mut symbols = vec![first; length];
    let mut rest = index;
    for symbol in symbols.iter_mut().rev() {
        if rest == 0 {
            break;
        }
        *symbol = first + (rest % base) as u8;
        rest /= base;
    }
    String::from_utf8(symbols).unwrap_or_default()
}

/// `value` and up to `more` values following it: the next whole numbers,
/// the other boolean, the text with a number after it (an empty text
/// followed by letters), an object or a list with one of its members or
/// items followed so.
fn after(value: &Value, more: usize) -> Vec<Value> {
    let mut found = vec![value.clone()];
    match value {
        Value::Number(number) => {
            if let Some(whole) = number.as_i64() {
                found.extend((1..=more as i64).map(|step| Value::from(whole.saturating_add(step))));
            } else if let Some(real) = number.as_f64() {
                found.extend((1..=more).map(|step| Value::from(real + step as f64)));
            }
        }
        Value::Bool(flag) => found.push(Value::Bool(!flag)),
        Value::String(text) if text.is_empty() => {
            found.extend((0..more).map(|index| Value::String(letters(index))));
        }
        Value::String(text) => {
            found.extend((2..more + 2).map(|index| Value::String(format!("{text}{index}"))));
        }
        Value::Object(members) => {
            for (key, member) in members {
                found.extend(after(member, more).into_iter().skip(1).map(|other| {
                    let mut changed = members.clone();
                    changed.insert(key.clone(), other);
                    Value::Object(changed)
                }));
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                found.extend(after(item, more).into_iter().skip(1).map(|other| {
                    let mut changed = items.clone();
                    changed[index] = other;
                    Value::Array(changed)
                }));
            }
        }
        _ => {}
    }
    found
}

/// The `index`th name in `a`, `b`, …, `z`, `aa`, `ab`, ….
fn letters(index: usize) -> String {
    let mut index = index;
    let mut name = Vec::new();
    loop {
        name.push(b'a' + (index % 26) as u8);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    name.reverse();
    String::from_utf8(name).unwrap_or_default()
}
