//! `false` schemas, read as the editor words them. The validator reports
//! `"additionalProperties": false` beside no `properties` as a false schema
//! on the object for its first key only ("False schema does not allow 1");
//! with an empty `properties` (which changes nothing) it names every key it
//! does not allow, as with `properties`. Any other false schema reads
//! "2 is not allowed".

use serde_json::{Map, Value};

use super::subschemas::each_schema_mut;

/// `schema` as the validator is built from: each object schema with
/// `"additionalProperties": false` and no `properties` or
/// `patternProperties` gets an empty `properties`. `None` when none needs it.
pub(crate) fn for_validation(schema: &Value) -> Option<Value> {
    let mut copy = schema.clone();
    let mut added = false;
    each_schema_mut(&mut copy, &mut |members| {
        if needs_properties(members) {
            members.insert("properties".to_string(), Value::Object(Map::new()));
            added = true;
        }
    });
    added.then_some(copy)
}

fn needs_properties(members: &Map<String, Value>) -> bool {
    members.get("additionalProperties") == Some(&Value::Bool(false))
        && !members.contains_key("properties")
        && !members.contains_key("patternProperties")
}

/// What a false schema says about `instance` (as JSON).
pub(crate) fn message(instance: &str) -> String {
    format!("{instance} is not allowed")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::for_validation;

    #[test]
    fn an_empty_properties_goes_beside_each_lone_additional_properties_false() {
        let schema = json!({
            "additionalProperties": false,
            "allOf": [
                {"additionalProperties": false},
                {"additionalProperties": false, "patternProperties": {"x": {}}}
            ],
            "$defs": {"a": {"additionalProperties": false, "properties": {"b": true}}},
            "const": {"additionalProperties": false},
            "enum": [{"additionalProperties": false}],
        });
        let mut expected = schema.clone();
        expected["properties"] = json!({});
        expected["allOf"][0]["properties"] = json!({});
        assert_eq!(for_validation(&schema), Some(expected));
        // A property named `additionalProperties` is a property's schema.
        let named = json!({"properties": {"additionalProperties": false}});
        assert_eq!(for_validation(&named), None);
    }
}
