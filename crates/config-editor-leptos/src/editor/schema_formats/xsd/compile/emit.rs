//! A content model as JSON Schema.

use std::collections::HashSet;

use serde_json::{Map, Value, json};

use super::{Compiler, Model};
use crate::editor::languages::XML_TEXT_KEY;

impl Compiler<'_> {
    /// `model` as JSON Schema.
    pub(super) fn model_schema(&self, model: Model) -> Value {
        let element_names: HashSet<&str> = model
            .props
            .iter()
            .filter(|(_, prop)| !prop.attribute)
            .map(|(key, _)| key.as_str())
            .collect();
        let mut properties = Map::new();
        let mut required = Vec::new();
        for (key, prop) in &model.props {
            // An attribute sharing its name with a child element is `@name`.
            let key = if prop.attribute && element_names.contains(key.as_str()) {
                format!("@{key}")
            } else {
                key.clone()
            };
            let repeated = prop.max.is_none_or(|max| max > 1);
            let schema = if repeated {
                let mut array = json!({"type": "array", "items": prop.schema});
                if prop.min > 1 {
                    array["minItems"] = Value::from(prop.min);
                }
                if let Some(max) = prop.max {
                    array["maxItems"] = Value::from(max);
                }
                array
            } else {
                prop.schema.clone()
            };
            if prop.min > 0 {
                required.push(Value::String(key.clone()));
            }
            properties.insert(key, schema);
        }
        // An attribute sharing its name with a child element reads as
        // `@name` only beside that element; without it, as `name`.
        for (key, prop) in &model.props {
            if prop.attribute
                && element_names.contains(key.as_str())
                && let Some(element) = properties.get_mut(key)
            {
                let either = json!({"anyOf": [element.clone(), prop.schema]});
                *element = either;
            }
        }
        let mut object = Map::new();
        object.insert("type".to_string(), Value::String("object".to_string()));
        if let Some(text) = &model.text {
            // Text beside the attributes.
            properties.insert(XML_TEXT_KEY.to_string(), text.value());
        } else if model.mixed {
            properties.insert(XML_TEXT_KEY.to_string(), json!({"type": "string"}));
        }
        object.insert("properties".to_string(), Value::Object(properties));
        if !required.is_empty() {
            object.insert("required".to_string(), Value::Array(required.clone()));
        }
        if !model.open && !self.settings.lax {
            if model.other_attributes {
                object.insert(
                    "patternProperties".to_string(),
                    json!({"^[^:@#]+:": {"xml": {"attribute": true}}}),
                );
            }
            object.insert("additionalProperties".to_string(), Value::Bool(false));
        }
        let mut choices: Vec<Value> = model
            .choices
            .iter()
            .map(|choice| {
                let mut options: Vec<Value> = choice
                    .options
                    .iter()
                    .map(|keys| json!({"required": keys}))
                    .collect();
                if !choice.required {
                    // Or none of them.
                    let any: Vec<Value> = options.clone();
                    options.push(json!({"not": {"anyOf": any}}));
                }
                json!({"oneOf": options})
            })
            .collect();
        match choices.len() {
            0 => {}
            1 => {
                if let Some(Value::Object(only)) = choices.pop() {
                    object.extend(only);
                }
            }
            _ => {
                object.insert("allOf".to_string(), Value::Array(choices));
            }
        }
        let object = Value::Object(object);
        match &model.text {
            // Simple content: the text alone when no attribute is written
            // (none is required), else the object.
            Some(text) if required.iter().all(|key| key == XML_TEXT_KEY) => {
                json!({"anyOf": [text.value(), object]})
            }
            _ if model.mixed && required.is_empty() => {
                json!({"anyOf": [{"type": "string"}, object]})
            }
            _ => object,
        }
    }
}
