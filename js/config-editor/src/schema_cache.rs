//! The schema the headless tools (`validate`, `convert`, `schemaWarnings`)
//! compiled last, kept: a host checking text after text against one schema
//! compiles it once, not on every call. One schema is kept, the last (by its
//! text, format, resources and options); another replaces it.

use std::cell::RefCell;

use config_editor_leptos::{Schema, SchemaError};

use crate::schema_formats::{SchemaSpec, SpecKey};

thread_local! {
    static LAST: RefCell<Option<(SpecKey, Schema)>> = const { RefCell::new(None) };
}

#[cfg(test)]
thread_local! {
    static COMPILES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// `text` in `spec`'s format, compiled: the one kept when it is the same
/// schema, else compiled (and kept, when valid).
pub(crate) fn compiled(text: &str, spec: &SchemaSpec) -> Result<Schema, SchemaError> {
    let key = spec.key(text);
    if let Some(schema) = LAST.with(|last| {
        last.borrow()
            .as_ref()
            .filter(|(known, _)| *known == key)
            .map(|(_, schema)| schema.clone())
    }) {
        return Ok(schema);
    }
    #[cfg(test)]
    COMPILES.with(|count| count.set(count.get() + 1));
    let schema = spec.compile(text)?;
    LAST.with(|last| *last.borrow_mut() = Some((key, schema.clone())));
    Ok(schema)
}

#[cfg(test)]
mod tests {
    use super::{COMPILES, compiled};
    use crate::schema_formats::SchemaSpec;

    /// Design review item 2: a schema checked against many times is
    /// compiled once; another schema text compiles again, and so does the
    /// same text with other resources.
    #[test]
    fn one_schema_used_again_and_again_compiles_once() {
        let compiles = || COMPILES.with(std::cell::Cell::get);
        let start = compiles();
        let schema = r#"{"properties":{"a":{"type":"integer"}}}"#;
        let json = SchemaSpec::json_schema();
        for _ in 0..100 {
            compiled(schema, &json).expect("valid");
        }
        assert_eq!(compiles() - start, 1);
        compiled(r#"{"type":"object"}"#, &json).expect("valid");
        compiled(schema, &json).expect("valid");
        assert_eq!(compiles() - start, 3);
        assert!(compiled("{", &json).is_err());
        assert_eq!(compiles() - start, 4);
        let mut with_resources = SchemaSpec::json_schema();
        with_resources.resources.insert("other.json", "{}");
        compiled(schema, &with_resources).expect("valid");
        compiled(schema, &with_resources).expect("valid");
        assert_eq!(compiles() - start, 5);
    }
}
