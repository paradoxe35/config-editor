//! The least list a list schema accepts, as an inserted block writes it:
//! as many items as its `minItems` (and its `contains`'s `minContains`)
//! asks, the first ones matching its `contains` and no more of them than
//! its `maxContains` allows, each distinct from the others when it asks
//! `uniqueItems` and the item schema allows another value (an enum's next
//! member, the next number or multiple, other text of its least length or
//! of its pattern's letters, an object or list with one of those inside),
//! so the list is no error the moment it is written.

use super::fill::{Filler, MAX_FILL_DEPTH, RESERVED_ITEMS};
use super::*;

/// The least list `schema` (a list's schema) accepts, each item filled as
/// a required value is (a list in an item, a list of lists, the same way);
/// empty when it asks none or an item has no value to write; as many
/// items as the fill may still write when it asks more, one at least.
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
    // Past the bound, a list asking items gets one from the reserve, so a
    // language writing a list as repeated keys still writes it.
    let open = filler.items_left.saturating_sub(RESERVED_ITEMS);
    let most = if open > 0 {
        open
    } else {
        filler.items_left.min(1)
    };
    // The items matching `contains` come first; where the `prefixItems`
    // there refuse them (`[{type: array}]` beside `contains: integer`),
    // after those.
    let prefix = array
        .get("prefixItems")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let starts = if contains.is_some() && prefix > 0 {
        vec![0, prefix]
    } else {
        vec![0]
    };
    let left = filler.items_left;
    let mut first = None;
    for from in starts {
        filler.items_left = left;
        let count = size("minItems")
            .unwrap_or(0)
            .max(from + containing)
            .min(most);
        filler.items_left -= count;
        let list = List {
            array,
            contains,
            containing,
            from,
            max_contains: contains.and(size("maxContains")),
            unique: array.get("uniqueItems") == Some(&Value::Bool(true)),
        };
        let mut items = Vec::with_capacity(count);
        for index in 0..count {
            match list.item(filler, index, &items, depth) {
                Some(item) => items.push(item),
                None => return empty(),
            }
        }
        let items = Value::Array(items);
        if decides(array, &items, filler.root) != Some(false) {
            return items;
        }
        first.get_or_insert(items);
    }
    first.unwrap_or_else(empty)
}

/// What a list schema asks of its items.
struct List<'a> {
    array: &'a Value,
    contains: Option<&'a Value>,
    /// How many items match `contains`, from the item at `from` on.
    containing: usize,
    from: usize,
    /// How many items may match `contains` at most.
    max_contains: Option<usize>,
    unique: bool,
}

impl List<'_> {
    /// Whether the item at `index` is one written to match `contains`.
    fn matching(&self, index: usize) -> bool {
        (self.from..self.from + self.containing).contains(&index)
    }

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
        let contains = self.contains.filter(|_| self.matching(index));
        let written = item.or(contains)?;
        let root = filler.root;
        // Past the items matching `contains`, one more match would be
        // more than `maxContains` allows.
        let avoided = self.contains.filter(|schema| {
            !self.matching(index)
                && self.max_contains.is_some_and(|most| {
                    items
                        .iter()
                        .filter(|item| decides(schema, item, root) == Some(true))
                        .count()
                        >= most
                })
        });
        let allowed = |value: &Value| {
            [item, contains]
                .into_iter()
                .flatten()
                .all(|schema| decides(schema, value, root) != Some(false))
                && avoided.is_none_or(|schema| decides(schema, value, root) == Some(false))
                && !(self.unique && items.contains(value))
        };
        let mut filled = Vec::new();
        for schema in [contains, item].into_iter().flatten() {
            if let Some(value) = filler.filled(schema, None, depth + 1) {
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
