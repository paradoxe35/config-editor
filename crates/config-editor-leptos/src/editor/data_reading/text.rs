//! Is `8080` text or a number: the readings of a scalar written as text
//! (XML). Without a schema, text a number writes back unchanged is that
//! number ([`plain_scalar`]); under a schema, text in any spelling a reader
//! takes for a number, boolean or `null` may be one ([`text_literal`],
//! decided by `conform`).

use crate::editor::applies::scalar_text;
use serde_json::Value;

/// Text read with no schema: `true`, `false` and `null`, and numbers that
/// write back as the same text, are those values; everything else is a
/// string. (`1e3`, `1.50`, `-0` and ` 4 ` stay text, so saving them keeps
/// what was typed; a schema wanting a number reads them as one.)
#[cfg(feature = "xml")]
pub(crate) fn plain_scalar(text: &str) -> Value {
    match text {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        _ => match serde_json::from_str::<Value>(text) {
            Ok(Value::Number(number)) if number.to_string() == text => Value::Number(number),
            _ => Value::String(text.to_string()),
        },
    }
}

/// The number, boolean or `null` XML text writes in any spelling a reader
/// takes for one (`1.50`, `.5`, `1e3`, `007`, `+4`, ` true `).
pub(crate) fn text_literal(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    match serde_json::from_str::<Value>(trimmed) {
        Ok(value @ (Value::Bool(_) | Value::Null)) => Some(value),
        _ => crate::editor::decimal_text::decimal(trimmed, true),
    }
}

/// A scalar of XML text read the other way: text as the number, boolean
/// or `null` it writes, or such a value as its text. (An empty element
/// stays an object: `<cloudflare/>` is the object it starts.)
pub(crate) fn other_reading(value: &Value) -> Option<Value> {
    match value {
        Value::String(text) => text_literal(text),
        Value::Number(_) | Value::Bool(_) | Value::Null => Some(Value::String(scalar_text(value))),
        _ => None,
    }
}
