//! Simple types: the built-in ones, and restrictions, lists and unions of
//! them, as JSON Schema for one text value (an attribute's, or a
//! text-only element's), with the hints the editor types the text by
//! (`x-true`, `x-false`, `x-whitespace`).

use serde_json::{Map, Value, json};

use super::super::super::completion::text_typing::number_of;

/// The JSON type a simple type's values read as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Kind {
    String,
    Integer,
    Number,
    Boolean,
    /// Several (a union), or any text.
    Any,
}

/// A simple type as JSON Schema.
#[derive(Clone, Debug)]
pub(super) struct Simple {
    pub(super) kind: Kind,
    pub(super) schema: Map<String, Value>,
}

impl Simple {
    fn of(kind: Kind, schema: Value) -> Self {
        let Value::Object(schema) = schema else {
            unreachable!("a schema object")
        };
        Simple { kind, schema }
    }

    /// The schema as a value.
    pub(super) fn value(&self) -> Value {
        Value::Object(self.schema.clone())
    }

    /// A value of this type written as `text` (an enumeration, a default),
    /// as the editor reads the text: a number or boolean where the type is
    /// one, else the text.
    pub(super) fn typed(&self, text: &str) -> Value {
        let trimmed = text.trim();
        match self.kind {
            Kind::Integer | Kind::Number => number_of(trimmed, self.kind == Kind::Number)
                .map_or_else(|| Value::String(text.to_string()), Value::Number),
            Kind::Boolean => match trimmed {
                "true" | "1" => Value::Bool(true),
                "false" | "0" => Value::Bool(false),
                _ => Value::String(text.to_string()),
            },
            Kind::String
                if self.schema.get("x-whitespace").and_then(Value::as_str) == Some("collapse") =>
            {
                Value::String(trimmed.split_whitespace().collect::<Vec<_>>().join(" "))
            }
            _ => Value::String(text.to_string()),
        }
    }
}

/// Integer bounds of the built-in integer types.
fn integer(minimum: Option<i128>, maximum: Option<i128>) -> Simple {
    let mut schema = json!({"type": "integer"});
    if let Some(minimum) = minimum {
        schema["minimum"] = bound(minimum);
    }
    if let Some(maximum) = maximum {
        schema["maximum"] = bound(maximum);
    }
    Simple::of(Kind::Integer, schema)
}

fn bound(value: i128) -> Value {
    i64::try_from(value)
        .map(Value::from)
        .or_else(|_| u64::try_from(value).map(Value::from))
        .unwrap_or(Value::Null)
}

/// A string type whose whitespace XSD collapses, with a pattern.
fn token(pattern: Option<&str>) -> Simple {
    let mut schema = json!({"type": "string", "x-whitespace": "collapse"});
    if let Some(pattern) = pattern {
        schema["pattern"] = Value::String(pattern.to_string());
    }
    Simple::of(Kind::String, schema)
}

/// The built-in type `name` of the XSD namespace; `None` for no such type.
/// The second value says what it loses, if anything.
pub(super) fn built_in(name: &str) -> Option<(Simple, Option<&'static str>)> {
    let name_pattern = |pattern: &str| super::regex::translate(pattern).ok();
    let string = |format: Option<&str>| {
        let mut schema = json!({"type": "string", "x-whitespace": "collapse"});
        if let Some(format) = format {
            schema["format"] = Value::String(format.to_string());
        }
        Simple::of(Kind::String, schema)
    };
    let pattern = |pattern: &str| {
        Simple::of(
            Kind::String,
            json!({"type": "string", "x-whitespace": "collapse", "pattern": pattern}),
        )
    };
    let simple = match name {
        "string" => Simple::of(Kind::String, json!({"type": "string"})),
        "normalizedString" => Simple::of(
            Kind::String,
            json!({"type": "string", "x-whitespace": "replace"}),
        ),
        "token" | "ENTITY" | "IDREF" | "ID" => token(None),
        "language" => token(Some("^[a-zA-Z]{1,8}(-[a-zA-Z0-9]{1,8})*$")),
        "Name" => token(name_pattern("\\i\\c*").as_deref()),
        "NCName" => token(name_pattern("[\\i-[:]][\\c-[:]]*").as_deref()),
        "QName" | "NOTATION" => {
            token(name_pattern("([\\i-[:]][\\c-[:]]*:)?[\\i-[:]][\\c-[:]]*").as_deref())
        }
        "NMTOKEN" => token(name_pattern("\\c+").as_deref()),
        "NMTOKENS" | "IDREFS" | "ENTITIES" => {
            return Some((token(None), Some("a list type's items are not checked")));
        }
        "boolean" => Simple::of(
            Kind::Boolean,
            json!({"type": "boolean", "x-true": ["true", "1"], "x-false": ["false", "0"]}),
        ),
        "decimal" => Simple::of(Kind::Number, json!({"type": "number"})),
        "float" | "double" => {
            return Some((
                Simple::of(Kind::Number, json!({"type": "number"})),
                Some("INF, -INF and NaN are not taken as numbers"),
            ));
        }
        "integer" => integer(None, None),
        "nonPositiveInteger" => integer(None, Some(0)),
        "negativeInteger" => integer(None, Some(-1)),
        "nonNegativeInteger" => integer(Some(0), None),
        "positiveInteger" => integer(Some(1), None),
        "long" => integer(Some(i64::MIN.into()), Some(i64::MAX.into())),
        "int" => integer(Some(i32::MIN.into()), Some(i32::MAX.into())),
        "short" => integer(Some(i16::MIN.into()), Some(i16::MAX.into())),
        "byte" => integer(Some(i8::MIN.into()), Some(i8::MAX.into())),
        "unsignedLong" => integer(Some(0), Some(u64::MAX.into())),
        "unsignedInt" => integer(Some(0), Some(u32::MAX.into())),
        "unsignedShort" => integer(Some(0), Some(u16::MAX.into())),
        "unsignedByte" => integer(Some(0), Some(u8::MAX.into())),
        "dateTime" => string(Some("date-time")),
        "date" => string(Some("date")),
        "time" => string(Some("time")),
        "duration" => string(Some("duration")),
        "gYear" => pattern("^-?[0-9]{4,}(Z|[+-][0-9]{2}:[0-9]{2})?$"),
        "gYearMonth" => pattern("^-?[0-9]{4,}-[0-9]{2}(Z|[+-][0-9]{2}:[0-9]{2})?$"),
        "gMonth" => pattern("^--[0-9]{2}(Z|[+-][0-9]{2}:[0-9]{2})?$"),
        "gMonthDay" => pattern("^--[0-9]{2}-[0-9]{2}(Z|[+-][0-9]{2}:[0-9]{2})?$"),
        "gDay" => pattern("^---[0-9]{2}(Z|[+-][0-9]{2}:[0-9]{2})?$"),
        "hexBinary" => pattern("^([0-9a-fA-F]{2})*$"),
        "base64Binary" => pattern("^[A-Za-z0-9+/= \\n\\r\\t]*$"),
        "anyURI" => string(Some("uri-reference")),
        "anySimpleType" | "anyAtomicType" => Simple::of(Kind::Any, json!({})),
        _ => return None,
    };
    Some((simple, None))
}
