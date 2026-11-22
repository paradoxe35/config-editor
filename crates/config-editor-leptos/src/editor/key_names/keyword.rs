//! The `x-config-editor-keys` keyword: a string schema whose value names a
//! key of a map elsewhere (`"/clients"`), read into the places it names.
//! A suggestion only: the validator ignores it, and a malformed value is
//! no place at all, never a warning.

use serde_json::Value;

/// The keyword's name in a schema.
pub(crate) const KEYWORD: &str = "x-config-editor-keys";

/// One place the keyword names: the map at `pointer` (a JSON pointer) of
/// the documents whose schema is this one, or one of `from` (other
/// schemas, by `$id`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KeyPlace {
    pub(crate) pointer: String,
    pub(crate) from: Vec<String>,
}

/// Every pointer the keyword names anywhere in `schema` (each once, in
/// the order found): the maps whose keys an editor keeps from the last
/// text that parsed.
pub(crate) fn pointers_in(schema: &Value) -> Vec<String> {
    let mut pointers: Vec<String> = Vec::new();
    crate::editor::schema::each_schema(schema, &(), &mut |_, members, ()| {
        if let Some(keyword) = members.get(KEYWORD) {
            let schema = serde_json::json!({ KEYWORD: keyword });
            for place in key_places(&schema) {
                if !pointers.contains(&place.pointer) {
                    pointers.push(place.pointer);
                }
            }
        }
        Some(())
    });
    pointers
}

/// The places `schema`'s keyword names, in the order written: a string
/// (`"/clients"`), a list of strings, an object with `pointer` and `from`
/// (a string or a list), or a list of such objects (strings and objects
/// may mix). An entry that is none of these, or a pointer that is no JSON
/// pointer (not empty and not starting with `/`), is left out.
pub(crate) fn key_places(schema: &Value) -> Vec<KeyPlace> {
    let Some(keyword) = schema.get(KEYWORD) else {
        return Vec::new();
    };
    match keyword {
        Value::Array(entries) => entries.iter().filter_map(place_of).collect(),
        entry => place_of(entry).into_iter().collect(),
    }
}

/// The place one entry names.
fn place_of(entry: &Value) -> Option<KeyPlace> {
    let (pointer, from) = match entry {
        Value::String(pointer) => (pointer.as_str(), Vec::new()),
        Value::Object(fields) => {
            let pointer = fields.get("pointer")?.as_str()?;
            let from = match fields.get("from") {
                None => Vec::new(),
                Some(Value::String(id)) => vec![id.clone()],
                Some(Value::Array(ids)) => ids
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
                Some(_) => return None,
            };
            (pointer, from)
        }
        _ => return None,
    };
    (pointer.is_empty() || pointer.starts_with('/')).then(|| KeyPlace {
        pointer: pointer.to_string(),
        from,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn place(pointer: &str, from: &[&str]) -> KeyPlace {
        KeyPlace {
            pointer: pointer.to_string(),
            from: from.iter().map(|id| id.to_string()).collect(),
        }
    }

    #[test]
    fn every_written_form_names_its_places() {
        let cases = [
            (json!("/clients"), vec![place("/clients", &[])]),
            (
                json!(["/clients", "/groups"]),
                vec![place("/clients", &[]), place("/groups", &[])],
            ),
            (
                json!({"pointer": "/clients", "from": "https://x/server"}),
                vec![place("/clients", &["https://x/server"])],
            ),
            (
                json!({"pointer": "/clients", "from": ["https://x/a", "https://x/b"]}),
                vec![place("/clients", &["https://x/a", "https://x/b"])],
            ),
            (
                json!([{"pointer": "/a"}, {"pointer": "/b", "from": ["s"]}, "/c"]),
                vec![place("/a", &[]), place("/b", &["s"]), place("/c", &[])],
            ),
            (json!(""), vec![place("", &[])]),
        ];
        for (keyword, expected) in cases {
            let schema = json!({ "type": "string", KEYWORD: keyword });
            assert_eq!(key_places(&schema), expected, "{keyword}");
        }
    }

    #[test]
    fn a_malformed_entry_names_no_place() {
        for keyword in [
            json!(3),
            json!(null),
            json!("clients"),
            json!({"from": "s"}),
            json!({"pointer": 1}),
            json!({"pointer": "/a", "from": 2}),
            json!([1, {"pointer": true}]),
        ] {
            let schema = json!({ KEYWORD: keyword });
            assert_eq!(key_places(&schema), Vec::new(), "{keyword}");
        }
        assert_eq!(key_places(&json!({"type": "string"})), Vec::new());
    }
}
