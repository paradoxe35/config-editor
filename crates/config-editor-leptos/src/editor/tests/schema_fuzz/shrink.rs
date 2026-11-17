//! Shrinking a failing case: its schema and documents made smaller (an
//! entry or item left out, a value replaced by one inside it, a scalar
//! made plainer) for as long as it fails the same way.

use serde_json::{Value, json};

use super::runner::Case;

/// `case` made as small as it can be (in at most `budget` checks) while
/// `fails_alike` says it still fails the way it did.
pub(super) fn shrink(case: Case, budget: usize, fails_alike: &dyn Fn(&Case) -> bool) -> Case {
    let mut current = case;
    let mut checks = 0;
    'smaller: loop {
        for field in 0..3 {
            let value = field_of(&current, field);
            for candidate in candidates(value) {
                checks += 1;
                if checks > budget {
                    return current;
                }
                let mut next = current.clone();
                *field_of_mut(&mut next, field) = candidate;
                if fails_alike(&next) {
                    current = next;
                    continue 'smaller;
                }
            }
        }
        // A smaller pick.
        if current.pick > 0 {
            let mut next = current.clone();
            next.pick /= 2;
            if fails_alike(&next) {
                current = next;
                continue 'smaller;
            }
        }
        return current;
    }
}

fn field_of(case: &Case, field: usize) -> &Value {
    match field {
        0 => &case.doc,
        1 => &case.other,
        _ => &case.schema,
    }
}

fn field_of_mut(case: &mut Case, field: usize) -> &mut Value {
    match field {
        0 => &mut case.doc,
        1 => &mut case.other,
        _ => &mut case.schema,
    }
}

/// Every one-step smaller `value`: at each node, an entry or item left
/// out, the node replaced by a child, a scalar made plainer; the ones
/// nearer the root first.
pub(super) fn candidates(value: &Value) -> Vec<Value> {
    let mut paths = Vec::new();
    nodes(value, &mut Vec::new(), &mut paths);
    let mut out = Vec::new();
    for path in &paths {
        let node = at(value, path);
        let mut variants: Vec<Value> = Vec::new();
        match node {
            Value::Object(map) => {
                for key in map.keys() {
                    let mut smaller = map.clone();
                    smaller.remove(key);
                    variants.push(Value::Object(smaller));
                }
                variants.extend(map.values().filter(|child| child.is_object()).cloned());
            }
            Value::Array(items) => {
                for index in 0..items.len() {
                    let mut smaller = items.clone();
                    smaller.remove(index);
                    variants.push(Value::Array(smaller));
                }
                variants.extend(items.iter().cloned());
            }
            // Never emptied: an empty key or name is no case worth reading.
            Value::String(text) if text.chars().count() > 1 => {
                variants.push(json!(text.chars().take(1).collect::<String>()));
            }
            Value::Number(number) if number.as_i64() != Some(0) => {
                variants.push(json!(0));
                if number.as_i64().is_none_or(|n| n.abs() > 1) {
                    variants.push(json!(1));
                }
            }
            Value::Bool(true) => variants.push(json!(false)),
            _ => {}
        }
        for variant in variants {
            let mut whole = value.clone();
            *at_mut(&mut whole, path) = variant;
            out.push(whole);
        }
    }
    out
}

enum Step {
    Key(String),
    Index(usize),
}

fn nodes(value: &Value, path: &mut Vec<Step>, out: &mut Vec<Vec<Step>>) {
    out.push(
        path.iter()
            .map(|step| match step {
                Step::Key(key) => Step::Key(key.clone()),
                Step::Index(index) => Step::Index(*index),
            })
            .collect(),
    );
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                path.push(Step::Key(key.clone()));
                nodes(child, path, out);
                path.pop();
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                path.push(Step::Index(index));
                nodes(child, path, out);
                path.pop();
            }
        }
        _ => {}
    }
}

fn at<'v>(value: &'v Value, path: &[Step]) -> &'v Value {
    path.iter().fold(value, |node, step| match step {
        Step::Key(key) => &node[key.as_str()],
        Step::Index(index) => &node[*index],
    })
}

fn at_mut<'v>(value: &'v mut Value, path: &[Step]) -> &'v mut Value {
    path.iter().fold(value, |node, step| match step {
        Step::Key(key) => &mut node[key.as_str()],
        Step::Index(index) => &mut node[*index],
    })
}
