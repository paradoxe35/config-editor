//! Numbers JSON cannot hold: infinities and NaN (YAML `.inf`, `-.inf`,
//! `.nan`; TOML `inf`, `-inf`, `nan`).
//!
//! The shared data model is a `serde_json::Value`, whose numbers are finite.
//! A non-finite number is held there as a string of one Unicode
//! noncharacter (U+FDD0, which Unicode reserves for a program's internal use
//! and never for interchanged text) followed by `inf`, `-inf` or `nan`:
//! - it survives every step that copies, compares or diffs values, so a
//!   conversion, an Edit-as save and the read-only check see it as the value
//!   it is (`inf` stays `inf`, never `null`);
//! - YAML and TOML write it back as their own number
//!   ([`ConfigLanguage::writes_non_finite`]); a language without one (JSON,
//!   HCL, XML) writes the text `inf`, which reads back as a string, so the
//!   conversion reports a loss and a save into that language waits;
//! - schema validation sees a number (beyond every finite bound);
//! - JavaScript sees `Infinity`, `-Infinity` and `NaN`.
//!
//! [`float_value`] and [`float_of`] build and read it.
//!
//! [`ConfigLanguage::writes_non_finite`]: super::super::language::ConfigLanguage::writes_non_finite

use std::borrow::Cow;
use std::fmt;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};
use serde_json::{Map, Number, Value};

/// The noncharacter starting a non-finite number's string.
const MARK: char = '\u{FDD0}';

/// A number in the data model: a JSON number when finite, else the value
/// standing for an infinity or NaN.
///
/// ```
/// use config_editor_leptos::{float_of, float_value};
/// assert_eq!(float_value(1.5), serde_json::json!(1.5));
/// assert_eq!(float_of(&float_value(f64::INFINITY)), Some(f64::INFINITY));
/// ```
#[must_use]
pub fn float_value(number: f64) -> Value {
    if let Some(finite) = Number::from_f64(number) {
        return Value::Number(finite);
    }
    let name = if number.is_nan() {
        "nan"
    } else if number > 0.0 {
        "inf"
    } else {
        "-inf"
    };
    Value::String(format!("{MARK}{name}"))
}

/// The number `value` holds, infinities and NaN included (see
/// [`float_value`]); `None` for anything but a number.
#[must_use]
pub fn float_of(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        _ => non_finite(value),
    }
}

/// The infinity or NaN `value` stands for.
pub(crate) fn non_finite(value: &Value) -> Option<f64> {
    match name(value)? {
        "inf" => Some(f64::INFINITY),
        "-inf" => Some(f64::NEG_INFINITY),
        _ => Some(f64::NAN),
    }
}

/// `inf`, `-inf` or `nan` for a non-finite number.
pub(crate) fn name(value: &Value) -> Option<&str> {
    let Value::String(text) = value else {
        return None;
    };
    text.strip_prefix(MARK)
        .filter(|name| matches!(*name, "inf" | "-inf" | "nan"))
}

/// Whether `value` holds a non-finite number anywhere.
pub(crate) fn contains(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().any(contains),
        Value::Object(map) => map.values().any(contains),
        _ => name(value).is_some(),
    }
}

/// `value` with each non-finite number replaced by `replace` of it.
fn replaced<'a>(value: &'a Value, replace: &dyn Fn(&str) -> Value) -> Cow<'a, Value> {
    if !contains(value) {
        return Cow::Borrowed(value);
    }
    fn walk(value: &Value, replace: &dyn Fn(&str) -> Value) -> Value {
        match value {
            Value::Array(items) => {
                Value::Array(items.iter().map(|item| walk(item, replace)).collect())
            }
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(key, item)| (key.clone(), walk(item, replace)))
                    .collect(),
            ),
            _ => name(value).map_or_else(|| value.clone(), replace),
        }
    }
    Cow::Owned(walk(value, replace))
}

/// `value` as a language without infinities writes it: each one as the text
/// `inf`, `-inf` or `nan` (which reads back as a string: a loss).
pub(crate) fn as_text(value: &Value) -> Cow<'_, Value> {
    replaced(value, &|name| Value::String(name.to_string()))
}

/// The finite numbers schema validation sees for `inf`, `-inf` and `nan`:
/// an infinity is the largest finite number of its sign (beyond every bound
/// a schema can write), NaN the one just below the largest.
fn stand_in(name: &str) -> f64 {
    match name {
        "inf" => f64::MAX,
        "-inf" => f64::MIN,
        _ => f64::from_bits(f64::MAX.to_bits() - 1),
    }
}

/// `value` as schema validation sees it: each non-finite number as its
/// stand-in, a number, as it is in the config.
pub(crate) fn for_validation(value: &Value) -> Cow<'_, Value> {
    replaced(value, &|name| {
        Number::from_f64(stand_in(name)).map_or(Value::Null, Value::Number)
    })
}

/// A validation message with the stand-ins it quotes written as the
/// numbers they stand for.
pub(crate) fn restore_in_message(message: &str) -> String {
    let mut message = message.to_string();
    for name in ["-inf", "inf", "nan"] {
        let shown = Value::from(stand_in(name)).to_string();
        message = message.replace(&shown, name);
    }
    message
}

/// `value` serialized with its non-finite numbers as floats, for a serde
/// writer that has them (YAML's `.inf`, TOML's `inf`).
pub(crate) struct Floats<'a>(pub(crate) &'a Value);

impl Serialize for Floats<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Value::Array(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(&Floats(item))?;
                }
                seq.end()
            }
            Value::Object(map) => {
                let mut out = serializer.serialize_map(Some(map.len()))?;
                for (key, item) in map {
                    out.serialize_entry(key, &Floats(item))?;
                }
                out.end()
            }
            value => match non_finite(value) {
                Some(number) => serializer.serialize_f64(number),
                None => value.serialize(serializer),
            },
        }
    }
}

/// A value read by a serde parser, its non-finite floats kept (a plain
/// `Value` turns them into `null`).
pub(crate) struct Parsed(pub(crate) Value);

impl<'de> Deserialize<'de> for Parsed {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ParsedVisitor).map(Parsed)
    }
}

struct ParsedVisitor;

impl<'de> Visitor<'de> for ParsedVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any value")
    }

    fn visit_bool<E>(self, flag: bool) -> Result<Value, E> {
        Ok(Value::Bool(flag))
    }

    fn visit_i64<E>(self, number: i64) -> Result<Value, E> {
        Ok(Value::from(number))
    }

    fn visit_u64<E>(self, number: u64) -> Result<Value, E> {
        Ok(Value::from(number))
    }

    fn visit_i128<E: de::Error>(self, number: i128) -> Result<Value, E> {
        i64::try_from(number)
            .map(Value::from)
            .or_else(|_| u64::try_from(number).map(Value::from))
            .map_err(|_| E::custom(format!("{number} is out of range")))
    }

    fn visit_u128<E: de::Error>(self, number: u128) -> Result<Value, E> {
        u64::try_from(number)
            .map(Value::from)
            .map_err(|_| E::custom(format!("{number} is out of range")))
    }

    fn visit_f64<E>(self, number: f64) -> Result<Value, E> {
        Ok(float_value(number))
    }

    fn visit_str<E>(self, text: &str) -> Result<Value, E> {
        Ok(Value::String(text.to_string()))
    }

    fn visit_string<E>(self, text: String) -> Result<Value, E> {
        Ok(Value::String(text))
    }

    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        Parsed::deserialize(deserializer).map(|parsed| parsed.0)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(Parsed(item)) = seq.next_element()? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Value, A::Error> {
        let mut map = Map::new();
        while let Some((key, Parsed(item))) = access.next_entry::<String, Parsed>()? {
            map.insert(key, item);
        }
        Ok(Value::Object(map))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_finite_numbers_round_trip_through_the_data_model() {
        for number in [f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(float_of(&float_value(number)), Some(number));
        }
        assert!(float_of(&float_value(f64::NAN)).is_some_and(f64::is_nan));
        assert_eq!(float_of(&Value::String("inf".into())), None);
        assert_eq!(float_of(&serde_json::json!(2)), Some(2.0));
        let value = serde_json::json!({"a": [float_value(f64::INFINITY)], "b": "x"});
        assert_eq!(
            *as_text(&value),
            serde_json::json!({"a": ["inf"], "b": "x"})
        );
        assert!(matches!(
            as_text(&serde_json::json!({"b": 1})),
            Cow::Borrowed(_)
        ));
    }
}
