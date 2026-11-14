//! The least list a list schema accepts, as an inserted block writes it:
//! as many items as its `minItems` (and its `contains`'s `minContains`)
//! asks, the first ones matching its `contains`, each distinct from the
//! others when it asks `uniqueItems` and the item schema allows another
//! value (an enum's next member, the next number, other text), so the list
//! is no error the moment it is written.

use super::conditions::decides;
use super::fill::{Filler, MAX_FILL_DEPTH};
use super::*;

/// The least list `schema` (a list's schema) accepts, each item filled as
/// a required value is (a list in an item, a list of lists, the same way);
/// empty when it asks none, an item has no value to write, or it asks more
/// items than the fill may still write.
pub(super) fn least_items(filler: &mut Filler<'_>, schema: &Value, depth: usize) -> Value {
    let empty = || Value::Array(Vec::new());
    let Some(array) = array_branch(schema, filler.root).filter(|_| depth < MAX_FILL_DEPTH) else {
        return empty();
    };
    let size = |keyword: &str| {
        array
            .get(keyword)
            .and_then(Value::as_u64)
            .map(|size| usize::try_from(size).unwrap_or(usize::MAX))
    };
    let contains = array.get("contains");
    let containing = contains.map_or(0, |_| size("minContains").unwrap_or(1));
    let count = size("minItems").unwrap_or(0).max(containing);
    if count > filler.items_left {
        return empty();
    }
    filler.items_left -= count;
    let list = List {
        array,
        contains,
        containing,
        unique: array.get("uniqueItems") == Some(&Value::Bool(true)),
    };
    let mut items = Vec::with_capacity(count);
    for index in 0..count {
        match list.item(filler, index, &items, depth) {
            Some(item) => items.push(item),
            None => return empty(),
        }
    }
    Value::Array(items)
}

/// What a list schema asks of its items.
struct List<'a> {
    array: &'a Value,
    contains: Option<&'a Value>,
    /// How many of the first items match `contains`.
    containing: usize,
    unique: bool,
}

impl List<'_> {
    /// The schema of the item at `index`.
    fn schema(&self, index: usize) -> Option<&Value> {
        self.array
            .get("prefixItems")
            .and_then(|prefix| prefix.get(index))
            .or_else(|| self.array.get("items").filter(|items| !items.is_array()))
    }

    /// The item at `index`, after `items`: the first value filled for it
    /// that the list allows there, else the first filled (an error the
    /// schema leaves no way around, such as more distinct items than an
    /// enum has members).
    fn item(
        &self,
        filler: &mut Filler<'_>,
        index: usize,
        items: &[Value],
        depth: usize,
    ) -> Option<Value> {
        let item = self.schema(index);
        let contains = self.contains.filter(|_| index < self.containing);
        let written = item.or(contains)?;
        let root = filler.root;
        let allowed = |value: &Value| {
            [item, contains]
                .into_iter()
                .flatten()
                .all(|schema| decides(schema, value, root) != Some(false))
                && !(self.unique && items.contains(value))
        };
        let mut filled = Vec::new();
        for schema in [contains, item].into_iter().flatten() {
            if let Some(value) = filler.filled(schema, depth + 1) {
                if allowed(&value) {
                    return Some(value);
                }
                filled.push(value);
            }
        }
        let first = filled.first().cloned();
        [contains, item]
            .into_iter()
            .flatten()
            .flat_map(|schema| others(schema, root, &filled, items.len() + 1))
            .find(|value| allowed(value) && filler.keeps(value, written))
            .or(first)
    }
}

/// Other values `schema` may allow than those `filled`: its enum members
/// and consts, its examples, and up to `more` values after each filled one
/// and after its lower bound (the next numbers, the other boolean, other
/// text).
fn others(schema: &Value, root: &Value, filled: &[Value], more: usize) -> Vec<Value> {
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
        if let Some(least) = branch.get("minimum").and_then(Value::as_i64) {
            found.extend(after(&Value::from(least), more + 1));
        }
        if let Some(above) = branch.get("exclusiveMinimum").and_then(Value::as_i64) {
            found.extend(after(&Value::from(above), more + 1).into_iter().skip(1));
        }
    }
    for value in filled {
        found.extend(after(value, more));
    }
    found
}

/// `value` and up to `more` values following it: the next whole numbers,
/// the other boolean, the text with a number after it (an empty text
/// followed by letters).
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
