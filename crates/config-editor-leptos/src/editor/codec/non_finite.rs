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
//! A real string spelled the same way (a document holding the text
//! `"\u{FDD0}inf"`) must stay a string, so the form is kept unforgeable:
//! every string a parser reads goes through [`string_value`], which puts one
//! more U+FDD0 in front of a string made of U+FDD0s and a name, and every
//! writer takes one off again ([`written`], [`string_of`]). Only those
//! strings carry the extra mark; every other string is held as it is.
//!
//! [`ConfigLanguage::writes_non_finite`]: super::super::language::ConfigLanguage::writes_non_finite

use std::borrow::Cow;
#[cfg(feature = "yaml")]
use std::fmt;

#[cfg(feature = "yaml")]
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
#[cfg(any(feature = "yaml", feature = "toml"))]
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};
#[cfg(feature = "yaml")]
use serde_json::Map;
use serde_json::{Number, Value};

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

/// A string in the data model, as a parser reads it: a string that would
/// read as a non-finite number's form ([`float_value`]) is held with one
/// more U+FDD0 in front, so it stays a string. Build a string with it
/// whenever its text may start with U+FDD0; [`string_of`] reads it back.
///
/// ```
/// use config_editor_leptos::{float_of, string_of, string_value};
/// let text = string_value("\u{FDD0}inf");
/// assert_eq!(float_of(&text), None);
/// assert_eq!(string_of(&text), Some("\u{FDD0}inf"));
/// assert_eq!(string_value("plain"), serde_json::json!("plain"));
/// ```
#[must_use]
pub fn string_value(text: impl Into<String>) -> Value {
    let text = text.into();
    if reserved_shape(&text) {
        Value::String(format!("{MARK}{text}"))
    } else {
        Value::String(text)
    }
}

/// The text of the string `value` holds, as a document writes it
/// ([`string_value`] undone); `None` for anything but a string, and for a
/// non-finite number.
#[must_use]
pub fn string_of(value: &Value) -> Option<&str> {
    match value {
        Value::String(_) if name(value).is_some() => None,
        Value::String(text) => Some(written(text)),
        _ => None,
    }
}

/// A string of the model as a writer writes it: one U+FDD0 taken off a
/// string made of U+FDD0s and a name (a non-finite number, held with one,
/// is written as its name, as [`as_text`] writes it). Every writer applies
/// it exactly once, to the model's own string.
pub(crate) fn written(text: &str) -> &str {
    if reserved_shape(text) {
        &text[MARK.len_utf8()..]
    } else {
        text
    }
}

/// Whether `text` is one or more U+FDD0 followed by `inf`, `-inf` or `nan`.
fn reserved_shape(text: &str) -> bool {
    let rest = text.trim_start_matches(MARK);
    rest.len() < text.len() && matches!(rest, "inf" | "-inf" | "nan")
}

/// `value` with each string read as [`string_value`] holds it: for a
/// parser that reads strings only (JSON, HCL, XML), applied to what it
/// read.
#[cfg(any(feature = "json", feature = "hcl", feature = "xml"))]
pub(crate) fn escape_strings(value: Value) -> Value {
    if !marked(&value) {
        return value;
    }
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(escape_strings).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, item)| (key, escape_strings(item)))
                .collect(),
        ),
        Value::String(text) => string_value(text),
        other => other,
    }
}

/// `value` with each string as a writer writes it ([`written`]), for a
/// serde writer that takes the whole value.
#[cfg(feature = "json")]
pub(crate) fn unescape_strings(value: &Value) -> Cow<'_, Value> {
    replaced(value, &|text| Value::String(written(text).to_string()))
}

/// Whether any string in `value` starts with U+FDD0.
fn marked(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().any(marked),
        Value::Object(map) => map.values().any(marked),
        Value::String(text) => text.starts_with(MARK),
        _ => false,
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
    name_of(text)
}

/// `value` with each string starting with U+FDD0 (a non-finite number, or
/// a string holding its form) replaced by `replace` of its text.
fn replaced<'a>(value: &'a Value, replace: &dyn Fn(&str) -> Value) -> Cow<'a, Value> {
    if !marked(value) {
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
            Value::String(text) if text.starts_with(MARK) => replace(text),
            _ => value.clone(),
        }
    }
    Cow::Owned(walk(value, replace))
}

/// `value` as a language without infinities writes it: each one as the text
/// `inf`, `-inf` or `nan` (which reads back as a string: a loss).
/// A string holding the form stays as the model holds it, for the
/// language's writer to write ([`written`]).
pub(crate) fn as_text(value: &Value) -> Cow<'_, Value> {
    replaced(value, &|text| match name_of(text) {
        Some(name) => Value::String(name.to_string()),
        None => Value::String(text.to_string()),
    })
}

/// [`name`] of a string's text.
fn name_of(text: &str) -> Option<&str> {
    text.strip_prefix(MARK)
        .filter(|name| matches!(*name, "inf" | "-inf" | "nan"))
}

#[cfg(feature = "schema-validation")]
/// Which finite numbers schema validation sees for `inf`, `-inf` and `nan`:
/// no one number fails every check a non-finite one fails, so a document
/// holding them is validated once per stand-in, and each pass adds the
/// checks it stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StandIn {
    /// Beyond every bound: an infinity is the largest finite number of its
    /// sign, NaN the one just below the largest (it fails every `maximum`).
    /// This pass is the validation; the others only add to it.
    Bounds,
    /// Fractions no schema writes a `multipleOf` of: an infinity or NaN is
    /// neither an integer nor a multiple of anything.
    Fractions,
    /// NaN as the one just above the smallest: it fails every `minimum` too.
    LowNan,
}

#[cfg(feature = "schema-validation")]
impl StandIn {
    /// Every pass, the validation itself first.
    pub(crate) const ALL: [StandIn; 3] = [StandIn::Bounds, StandIn::Fractions, StandIn::LowNan];

    /// The number standing for `name` (`inf`, `-inf`, `nan`) in this pass.
    fn number(self, name: &str) -> f64 {
        match (self, name) {
            (StandIn::Bounds | StandIn::LowNan, "inf") => f64::MAX,
            (StandIn::Bounds | StandIn::LowNan, "-inf") => f64::MIN,
            (StandIn::Bounds, _) => f64::from_bits(f64::MAX.to_bits() - 1),
            (StandIn::LowNan, _) => f64::from_bits(f64::MIN.to_bits() - 1),
            (StandIn::Fractions, "inf") => 0.123_456_789_012_345_67,
            (StandIn::Fractions, "-inf") => -0.123_456_789_012_345_67,
            (StandIn::Fractions, _) => 0.234_567_890_123_456_78,
        }
    }

    /// Whether an error of `kind` this pass finds at `instance` is one it
    /// stands for: at a stand-in, for the checks the pass is for (the bounds
    /// pass is the validation and stands for all).
    pub(crate) fn reports(
        self,
        instance: &Value,
        kind: &jsonschema::error::ValidationErrorKind,
    ) -> bool {
        use jsonschema::error::ValidationErrorKind as Kind;
        if self == StandIn::Bounds {
            return true;
        }
        let Some(number) = instance.as_f64() else {
            return false;
        };
        let Some(name) = ["inf", "-inf", "nan"]
            .into_iter()
            .find(|name| self.number(name).to_bits() == number.to_bits())
        else {
            return false;
        };
        match self {
            StandIn::Bounds => true,
            StandIn::Fractions => matches!(kind, Kind::Type { .. } | Kind::MultipleOf { .. }),
            StandIn::LowNan => {
                name == "nan"
                    && matches!(kind, Kind::Minimum { .. } | Kind::ExclusiveMinimum { .. })
            }
        }
    }
}

#[cfg(feature = "schema-validation")]
/// `value` as schema validation sees it in pass `stand_in`: each non-finite
/// number as its stand-in, a number, as it is in the config.
/// A string holding the form is validated as the text it is.
pub(crate) fn for_validation(value: &Value, stand_in: StandIn) -> Cow<'_, Value> {
    replaced(value, &|text| match name_of(text) {
        Some(name) => Number::from_f64(stand_in.number(name)).map_or(Value::Null, Value::Number),
        None => Value::String(written(text).to_string()),
    })
}

#[cfg(feature = "schema-validation")]
/// A validation message with the stand-ins it quotes written as the
/// numbers they stand for.
pub(crate) fn restore_in_message(message: &str) -> String {
    let mut shown: Vec<(String, &str)> = StandIn::ALL
        .iter()
        .flat_map(|pass| {
            ["-inf", "inf", "nan"].map(|name| (Value::from(pass.number(name)).to_string(), name))
        })
        .collect();
    // The longest first: `-1.79…e308` before the `1.79…e308` it holds.
    shown.sort_by_key(|(text, _)| std::cmp::Reverse(text.len()));
    shown.dedup();
    let mut message = message.to_string();
    for (text, name) in shown {
        message = message.replace(&text, name);
    }
    message
}

#[cfg(any(feature = "yaml", feature = "toml"))]
/// `value` serialized with its non-finite numbers as floats, for a serde
/// writer that has them (YAML's `.inf`, TOML's `inf`).
pub(crate) struct Floats<'a>(pub(crate) &'a Value);

#[cfg(any(feature = "yaml", feature = "toml"))]
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
            value => match (non_finite(value), value) {
                (Some(number), _) => serializer.serialize_f64(number),
                (None, Value::String(text)) => serializer.serialize_str(written(text)),
                (None, value) => value.serialize(serializer),
            },
        }
    }
}

#[cfg(feature = "yaml")]
/// A value read by a serde parser, its non-finite floats kept (a plain
/// `Value` turns them into `null`).
pub(crate) struct Parsed(pub(crate) Value);

#[cfg(feature = "yaml")]
impl<'de> Deserialize<'de> for Parsed {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ParsedVisitor).map(Parsed)
    }
}

#[cfg(feature = "yaml")]
struct ParsedVisitor;

#[cfg(feature = "yaml")]
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
        Ok(string_value(text))
    }

    fn visit_string<E>(self, text: String) -> Result<Value, E> {
        Ok(string_value(text))
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

    /// A tagged node (YAML `!Sub x`, `!GetAtt [a, b]`, `!include`): read as
    /// the value under the tag, which the text keeps.
    fn visit_enum<A: de::EnumAccess<'de>>(self, data: A) -> Result<Value, A::Error> {
        let (_tag, variant): (de::IgnoredAny, _) = data.variant()?;
        de::VariantAccess::newtype_variant::<Parsed>(variant).map(|parsed| parsed.0)
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
