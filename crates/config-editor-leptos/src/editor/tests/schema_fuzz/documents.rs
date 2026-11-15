//! Random documents for a schema: drawn to match it, with a chance at each
//! value of drawing anything instead, so about half the documents are
//! invalid somewhere (the reference says which).

use serde_json::{Map, Value, json};

use super::random::Rng;
use super::schemas::KEYS;

/// How a document is drawn.
#[derive(Clone, Copy)]
pub(super) struct Shape {
    /// Percent chance of drawing any value instead of a matching one.
    pub(super) wrong: usize,
    /// Secret fields get distinct values starting with [`SECRET`].
    pub(super) secrets: bool,
    /// Every property is written (a witness of each key's schema).
    pub(super) every_key: bool,
}

/// What every secret value starts with: a message, hover or suggestion
/// holding it shows a secret.
pub(super) const SECRET: &str = "sekret";

/// Text values drawn: some read as other types in YAML or XML text.
const TEXTS: &[&str] = &[
    "abc", "x", "", "8", "true", "1.5", "a b", "null", "x_y", "Zed", "fast", "08", "-3",
];

/// A random document for the root `schema`: an object.
pub(super) fn document(rng: &mut Rng, schema: &Value, shape: Shape) -> Value {
    let mut drawer = Drawer {
        rng,
        root: schema,
        shape,
        secrets: 0,
    };
    let value = drawer.value(schema, 0);
    match value {
        Value::Object(_) => value,
        other => json!({ "a": other }),
    }
}

struct Drawer<'a> {
    rng: &'a mut Rng,
    root: &'a Value,
    shape: Shape,
    secrets: usize,
}

/// The schema a local `$ref` names.
pub(super) fn resolve<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    root.pointer(reference.strip_prefix('#')?)
}

impl Drawer<'_> {
    fn value(&mut self, schema: &Value, depth: usize) -> Value {
        if depth > 7 {
            return json!("s");
        }
        if depth > 0 && self.rng.chance(self.shape.wrong) {
            return self.any(depth);
        }
        let Some(node) = schema.as_object() else {
            return self.any(depth);
        };
        if let Some(constant) = node.get("const") {
            return constant.clone();
        }
        if let Some(Value::Array(members)) = node.get("enum")
            && !members.is_empty()
        {
            return self.rng.pick(members).clone();
        }
        // Every part that applies: the node itself, the referenced schema,
        // one option of each `oneOf`/`anyOf`, every `allOf` member, the
        // `then` or the `else`.
        let mut parts: Vec<&Value> = Vec::new();
        if let Some(Value::String(reference)) = node.get("$ref")
            && let Some(target) = resolve(self.root, reference)
        {
            parts.push(target);
        }
        for keyword in ["oneOf", "anyOf"] {
            if let Some(Value::Array(options)) = node.get(keyword)
                && !options.is_empty()
            {
                parts.push(self.rng.pick(options));
            }
        }
        if let Some(Value::Array(all)) = node.get("allOf") {
            parts.extend(all.iter());
        }
        let condition = if self.rng.chance(50) { "then" } else { "else" };
        parts.extend(node.get(condition));
        let own = self.own(node, depth);
        let mut values: Vec<Value> = own.into_iter().collect();
        for part in parts {
            values.push(self.value(part, depth + 1));
        }
        merged(values).unwrap_or_else(|| self.any(depth))
    }

    /// The value the node's own keywords describe; `None` when it says
    /// nothing of its value's type.
    fn own(&mut self, node: &Map<String, Value>, depth: usize) -> Option<Value> {
        let types: Vec<&str> = match node.get("type") {
            Some(Value::String(name)) => vec![name.as_str()],
            Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).collect(),
            _ if node.contains_key("properties") || node.contains_key("required") => {
                vec!["object"]
            }
            _ if node.contains_key("items") || node.contains_key("prefixItems") => vec!["array"],
            _ => return None,
        };
        let kind = *self.rng.pick(&types);
        Some(match kind {
            "object" => self.object(node, depth),
            "array" => self.array(node, depth),
            "string" => self.string(node),
            "integer" => self.integer(node),
            "number" => self.number(node),
            "boolean" => json!(self.rng.chance(50)),
            "null" => Value::Null,
            _ => self.any(depth),
        })
    }

    fn object(&mut self, node: &Map<String, Value>, depth: usize) -> Value {
        let mut map = Map::new();
        let required: Vec<&str> = node
            .get("required")
            .and_then(Value::as_array)
            .map(|keys| keys.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if let Some(Value::Object(properties)) = node.get("properties") {
            for (key, schema) in properties {
                if required.contains(&key.as_str()) || self.shape.every_key || self.rng.chance(60) {
                    let value = self.value(schema, depth + 1);
                    map.insert(key.clone(), value);
                }
            }
        }
        if let Some(Value::Object(patterns)) = node.get("patternProperties")
            && let Some(schema) = patterns.values().next()
            && self.rng.chance(40)
        {
            let value = self.value(schema, depth + 1);
            map.insert("x_one".to_string(), value);
        }
        for key in required {
            if !map.contains_key(key) && self.rng.chance(80) {
                map.insert(key.to_string(), self.any(depth + 1));
            }
        }
        if self.rng.chance(self.shape.wrong / 2) {
            map.insert("zz".to_string(), self.any(depth + 1));
        }
        Value::Object(map)
    }

    fn array(&mut self, node: &Map<String, Value>, depth: usize) -> Value {
        let mut items = Vec::new();
        if let Some(Value::Array(prefix)) = node.get("prefixItems") {
            for schema in prefix {
                if self.rng.chance(85) {
                    items.push(self.value(schema, depth + 1));
                }
            }
        }
        let least = node.get("minItems").and_then(Value::as_u64).unwrap_or(0) as usize;
        let count = least.max(items.len()) + self.rng.below(3);
        let schema = node.get("items").cloned().unwrap_or(json!({}));
        while items.len() < count && schema != json!(false) {
            items.push(self.value(&schema, depth + 1));
        }
        if let Some(contains) = node.get("contains")
            && self.rng.chance(70)
        {
            items.push(self.value(contains, depth + 1));
        }
        Value::Array(items)
    }

    fn string(&mut self, node: &Map<String, Value>) -> Value {
        let secret = node.get("writeOnly") == Some(&json!(true))
            || node.get("x-secret") == Some(&json!(true));
        if secret && self.shape.secrets {
            self.secrets += 1;
            return json!(format!("{SECRET}{}q{}", self.secrets, self.rng.below(1000)));
        }
        match node.get("pattern").and_then(Value::as_str) {
            Some("^[a-z]+$") if self.rng.chance(70) => json!("abc"),
            Some("^x") if self.rng.chance(70) => json!("xyz"),
            Some("[0-9]") if self.rng.chance(70) => json!("a1"),
            _ => json!(self.rng.pick(TEXTS)),
        }
    }

    fn integer(&mut self, node: &Map<String, Value>) -> Value {
        let least = node.get("minimum").and_then(Value::as_i64).unwrap_or(-2);
        let mut value = least + self.rng.below(8) as i64;
        if let Some(step) = node.get("multipleOf").and_then(Value::as_i64)
            && self.rng.chance(70)
            && step > 0
        {
            value -= value.rem_euclid(step);
        }
        json!(value)
    }

    fn number(&mut self, node: &Map<String, Value>) -> Value {
        if self.rng.chance(30) {
            return self.integer(node);
        }
        json!(*self.rng.pick(&[0.5, 1.5, 2.25, -0.75, 3.5, 10.0]))
    }

    /// Any value: a scalar, or a small object or list.
    fn any(&mut self, depth: usize) -> Value {
        match self.rng.below(if depth > 4 { 5 } else { 7 }) {
            0 => json!(self.rng.pick(TEXTS)),
            1 => json!(self.rng.below(20) as i64 - 3),
            2 => json!(*self.rng.pick(&[0.5, 2.5, -1.25])),
            3 => json!(self.rng.chance(50)),
            4 => Value::Null,
            5 => {
                let key = *self.rng.pick(KEYS);
                json!({ key: self.any(depth + 1) })
            }
            _ => json!([self.any(depth + 1)]),
        }
    }
}

/// The values of the parts that apply, as one: objects joined (the first
/// written value of a key kept), else the last value that says something.
fn merged(values: Vec<Value>) -> Option<Value> {
    if values.is_empty() {
        return None;
    }
    if values.iter().all(Value::is_object) {
        let mut joined = Map::new();
        for value in values {
            if let Value::Object(map) = value {
                for (key, child) in map {
                    joined.entry(key).or_insert(child);
                }
            }
        }
        return Some(Value::Object(joined));
    }
    values
        .into_iter()
        .rev()
        .find(|value| !value.is_object())
        .or(None)
}
