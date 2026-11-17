//! Random schemas: an object at the root, its fields drawn from every
//! feature the editor reads (see the module docs), a few levels deep, with
//! `$defs` that may refer to themselves.

use serde_json::{Map, Value, json};

use super::random::Rng;

/// The keys fields are named from: valid XML names, none YAML reads as
/// another type; `x_…` match the pattern properties.
pub(super) const KEYS: &[&str] = &[
    "a", "b", "c", "d", "port", "name", "mode", "size", "x_one", "x_two",
];

/// The pattern of `patternProperties`.
pub(super) const PATTERN: &str = "^x_";

/// What a drawn schema may hold besides the plain features.
#[derive(Clone, Copy, Default)]
pub(super) struct Draw {
    /// Secret fields (`writeOnly`, `x-secret`) and `readOnly` ones.
    pub(super) marks: bool,
    /// Annotations completion and hover read: titles, descriptions,
    /// defaults and examples.
    pub(super) docs: bool,
    /// How many levels of fields (2 or 3 keep cases small).
    pub(super) depth: usize,
}

/// A random root schema.
pub(super) fn schema(rng: &mut Rng, draw: Draw) -> Value {
    let mut drawer = Drawer {
        rng,
        draw,
        defs: Map::new(),
    };
    let mut root = drawer.object(0);
    if !drawer.defs.is_empty() {
        root["$defs"] = Value::Object(std::mem::take(&mut drawer.defs));
    }
    root
}

struct Drawer<'r> {
    rng: &'r mut Rng,
    draw: Draw,
    defs: Map<String, Value>,
}

impl Drawer<'_> {
    fn node(&mut self, depth: usize) -> Value {
        let deep = depth >= self.draw.depth.max(1);
        let weights: &[usize] = if deep {
            &[10, 0, 0, 0, 0, 2]
        } else {
            &[8, 4, 3, 3, 2, 2]
        };
        let mut node = match self.rng.weighted(weights) {
            0 => self.scalar(),
            1 => self.object(depth + 1),
            2 => self.array(depth + 1),
            3 => self.combined(depth + 1),
            4 => self.reference(depth + 1),
            _ => self.choice(),
        };
        self.annotate(&mut node);
        node
    }

    fn scalar(&mut self) -> Value {
        match self.rng.below(7) {
            0 | 1 => {
                let mut node = json!({"type": "string"});
                if self.rng.chance(25) {
                    node["minLength"] = json!(self.rng.below(3));
                }
                if self.rng.chance(15) {
                    node["maxLength"] = json!(2 + self.rng.below(4));
                }
                if self.rng.chance(20) {
                    node["pattern"] = json!(self.rng.pick(&["^[a-z]+$", "^x", "[0-9]", "^.{2,}$"]));
                }
                node
            }
            2 => {
                let mut node = json!({"type": "integer"});
                self.bounds(&mut node, &[2, 3, 5]);
                node
            }
            3 => {
                let mut node = json!({"type": "number"});
                self.bounds(&mut node, &[0.5, 0.25]);
                node
            }
            4 => json!({"type": "boolean"}),
            5 => json!({"type": self.rng.pick(&["null", "string", "integer"])}),
            _ => {
                let types: &[&[&str]] = &[
                    &["integer", "string"],
                    &["boolean", "string"],
                    &["number", "null"],
                    &["string", "null"],
                    &["integer", "boolean"],
                    &["number", "string"],
                ];
                json!({"type": self.rng.pick(types)})
            }
        }
    }

    fn bounds<T: Into<Value> + Copy>(&mut self, node: &mut Value, multiples: &[T]) {
        if self.rng.chance(30) {
            node["minimum"] = json!(self.rng.below(5) as i64 - 1);
        }
        if self.rng.chance(20) {
            node["maximum"] = json!(5 + self.rng.below(100));
        }
        if self.rng.chance(10) {
            node["exclusiveMinimum"] = json!(0);
        }
        if self.rng.chance(15) {
            node["multipleOf"] = (*self.rng.pick(multiples)).into();
        }
    }

    /// An `enum` or `const`, with members that look like other types
    /// (`"8"`, `"true"`) to try XML's typing by the schema.
    fn choice(&mut self) -> Value {
        let enums = [
            json!(["fast", "safe"]),
            json!([1, 2, 3]),
            json!(["a", 1, true, null]),
            json!(["8", "true", "1.5"]),
            json!([0, "0", false]),
            json!([1.5, 2]),
        ];
        let members = self.rng.pick(&enums).clone();
        if self.rng.chance(30) {
            let members = members.as_array().unwrap();
            json!({"const": self.rng.pick(members)})
        } else {
            json!({"enum": members})
        }
    }

    fn keys(&mut self, most: usize) -> Vec<&'static str> {
        let count = 1 + self.rng.below(most);
        let mut keys: Vec<&'static str> = Vec::new();
        while keys.len() < count {
            let key = *self.rng.pick(KEYS);
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        keys
    }

    fn object(&mut self, depth: usize) -> Value {
        let keys = self.keys(4);
        let mut properties = Map::new();
        for key in &keys {
            properties.insert((*key).to_string(), self.node(depth));
        }
        let mut node = json!({"type": "object"});
        if depth > 0 && self.rng.chance(15) {
            node.as_object_mut().unwrap().remove("type");
        }
        // A field whose type comes from how schemas combine, not from one
        // schema: the base properties only describe it (or allow a wider
        // type) and an `allOf`, or the `then` its object always takes,
        // gives the type.
        if self.rng.chance(15) {
            let key = keys[0].to_string();
            let typed = properties[&key].clone();
            let base = match (self.rng.below(2), typed.get("type").and_then(Value::as_str)) {
                (1, Some(name)) if name != "string" => json!({"type": [name, "string"]}),
                _ => json!({"description": "described"}),
            };
            properties.insert(key.clone(), base);
            node["allOf"] = if self.rng.chance(50) {
                json!([{"properties": {key: typed}}])
            } else {
                json!([{"if": {"type": "object"}, "then": {"properties": {key: typed}}}])
            };
        }
        node["properties"] = Value::Object(properties);
        let required: Vec<&str> = keys
            .iter()
            .copied()
            .filter(|_| self.rng.chance(35))
            .collect();
        if !required.is_empty() {
            node["required"] = json!(required);
        }
        match self.rng.below(10) {
            0 | 1 => node["additionalProperties"] = json!(false),
            2 => node["additionalProperties"] = self.leaf(),
            _ => {}
        }
        if self.rng.chance(15) {
            node["patternProperties"] = json!({ PATTERN: self.leaf() });
        }
        if self.rng.chance(12) && keys.len() > 1 {
            node["dependentSchemas"] = json!({keys[0]: if self.rng.chance(50) {
                json!({"required": [keys[1]]})
            } else {
                json!({"properties": {keys[1]: self.leaf()}})
            }});
        }
        if self.rng.chance(6) && keys.len() > 1 {
            node["dependentRequired"] = json!({keys[1]: [keys[0]]});
        }
        if self.rng.chance(12) && keys.len() > 1 {
            let when = self
                .rng
                .pick(&[json!("fast"), json!(1), json!(true)])
                .clone();
            node["if"] = json!({"properties": {keys[0]: {"const": when}}, "required": [keys[0]]});
            node["then"] = json!({"properties": {keys[1]: self.leaf()}});
            if self.rng.chance(60) {
                node["else"] = json!({"properties": {keys[1]: self.leaf()}});
            }
        }
        if self.rng.chance(8) {
            node["minProperties"] = json!(1);
        }
        if self.rng.chance(5) {
            node["maxProperties"] = json!(2 + self.rng.below(2));
        }
        node
    }

    /// A scalar or choice: the small schemas conditionals and patterns
    /// give.
    fn leaf(&mut self) -> Value {
        let mut leaf = if self.rng.chance(70) {
            self.scalar()
        } else {
            self.choice()
        };
        self.annotate(&mut leaf);
        leaf
    }

    fn array(&mut self, depth: usize) -> Value {
        let mut node = json!({"type": "array"});
        if self.rng.chance(25) {
            let prefix: Vec<Value> = (0..1 + self.rng.below(2))
                .map(|_| self.node(depth))
                .collect();
            node["prefixItems"] = json!(prefix);
            match self.rng.below(3) {
                0 => node["items"] = json!(false),
                1 => node["items"] = self.node(depth),
                _ => {}
            }
        } else {
            node["items"] = self.node(depth);
        }
        if self.rng.chance(15) {
            node["contains"] = self.leaf();
            if self.rng.chance(50) {
                node["minContains"] = json!(1 + self.rng.below(2));
            }
            if self.rng.chance(30) {
                node["maxContains"] = json!(1 + self.rng.below(2));
            }
        }
        if self.rng.chance(25) {
            node["minItems"] = json!(1 + self.rng.below(2));
        }
        if self.rng.chance(15) {
            node["maxItems"] = json!(2 + self.rng.below(3));
        }
        if self.rng.chance(15) {
            node["uniqueItems"] = json!(true);
        }
        node
    }

    fn combined(&mut self, depth: usize) -> Value {
        let keyword = *self
            .rng
            .pick(&["oneOf", "anyOf", "allOf", "oneOf", "anyOf"]);
        let options: Vec<Value> = (0..2 + self.rng.below(2))
            .map(|_| {
                // Objects are what options tell apart most often.
                if self.rng.chance(50) {
                    self.object(depth)
                } else {
                    self.node(depth)
                }
            })
            .collect();
        let mut node = json!({ keyword: options });
        if self.rng.chance(10) {
            node["not"] = self.leaf();
        }
        node
    }

    /// A reference to a new `$defs` entry, which may hold itself under a
    /// field (a tree), or to one drawn before.
    fn reference(&mut self, depth: usize) -> Value {
        // Not one being drawn (`{}` still): an `allOf` holding itself
        // would never end.
        let drawn: Vec<String> = self
            .defs
            .iter()
            .filter(|(_, def)| **def != json!({}))
            .map(|(name, _)| name.clone())
            .collect();
        if !drawn.is_empty() && self.rng.chance(40) {
            return json!({"$ref": format!("#/$defs/{}", self.rng.pick(&drawn))});
        }
        let name = format!("d{}", self.defs.len());
        // Reserved first, so a reference inside it finds it.
        self.defs.insert(name.clone(), json!({}));
        let mut def = if self.rng.chance(50) {
            self.object(depth)
        } else {
            self.node(depth)
        };
        if self.rng.chance(50) {
            let key = *self.rng.pick(&["c", "next"]);
            let link = json!({"$ref": format!("#/$defs/{name}")});
            let link = if self.rng.chance(50) {
                link
            } else {
                json!({"type": "array", "items": link})
            };
            if def.get("properties").is_some_and(Value::is_object) {
                def["properties"][key] = link;
            } else {
                def = json!({"anyOf": [def, {"type": "object", "properties": {key: link}}]});
            }
        }
        self.defs.insert(name.clone(), def);
        json!({"$ref": format!("#/$defs/{name}")})
    }

    fn annotate(&mut self, node: &mut Value) {
        if self.draw.docs {
            if self.rng.chance(25) {
                node["description"] = json!("the field's description");
            }
            if self.rng.chance(10) {
                node["title"] = json!("Title");
            }
            if self.rng.chance(15) {
                let defaults = [json!(1), json!("fast"), json!(true), json!(2.5), json!("8")];
                node["default"] = self.rng.pick(&defaults).clone();
            }
            if self.rng.chance(10) {
                let examples = [json!([3]), json!(["ex"]), json!([false]), json!(["8"])];
                node["examples"] = self.rng.pick(&examples).clone();
            }
            if self.rng.chance(3) {
                node["deprecated"] = json!(true);
            }
        }
        let string = node.get("type") == Some(&json!("string"));
        if self.draw.marks && string && self.rng.chance(60) {
            match self.rng.below(3) {
                0 => node["writeOnly"] = json!(true),
                1 => node["x-secret"] = json!(true),
                _ => node["readOnly"] = json!(true),
            }
        }
    }
}
