//! The schema the headless tools (`validate`, `convert`, `schemaWarnings`)
//! compiled last, kept: a host checking text after text against one schema
//! compiles it once, not on every call. One schema text is kept, the last;
//! another replaces it.

use std::cell::RefCell;

use config_editor_leptos::{Schema, SchemaError};

thread_local! {
    static LAST: RefCell<Option<(String, Schema)>> = const { RefCell::new(None) };
}

#[cfg(test)]
thread_local! {
    static COMPILES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The compiled `json`: the one kept when it is the same text, else
/// compiled (and kept, when valid).
pub(crate) fn compiled(json: &str) -> Result<Schema, SchemaError> {
    if let Some(schema) = LAST.with(|last| {
        last.borrow()
            .as_ref()
            .filter(|(text, _)| text == json)
            .map(|(_, schema)| schema.clone())
    }) {
        return Ok(schema);
    }
    #[cfg(test)]
    COMPILES.with(|count| count.set(count.get() + 1));
    let schema = Schema::compile(json)?;
    LAST.with(|last| *last.borrow_mut() = Some((json.to_string(), schema.clone())));
    Ok(schema)
}

#[cfg(test)]
mod tests {
    use super::{COMPILES, compiled};

    /// Design review item 2: a schema checked against many times is
    /// compiled once; another schema text compiles again.
    #[test]
    fn one_schema_used_again_and_again_compiles_once() {
        let compiles = || COMPILES.with(std::cell::Cell::get);
        let start = compiles();
        let schema = r#"{"properties":{"a":{"type":"integer"}}}"#;
        for _ in 0..100 {
            compiled(schema).expect("valid");
        }
        assert_eq!(compiles() - start, 1);
        compiled(r#"{"type":"object"}"#).expect("valid");
        compiled(schema).expect("valid");
        assert_eq!(compiles() - start, 3);
        assert!(compiled("{").is_err());
        assert_eq!(compiles() - start, 4);
    }
}
