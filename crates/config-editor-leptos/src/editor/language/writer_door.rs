//! The one door into a language's writers. The data model marks the values
//! JSON cannot hold (infinities and NaN, and a string spelled like one; see
//! `codec::non_finite`), and only the core knows those marks. Every call the
//! editor makes into a hook that writes a value's text goes through the
//! methods here, which give the language its values as it writes them:
//! plain data for a language without infinities (each string as its text,
//! each infinity or NaN as the text `inf`, `-inf` or `nan`), the model's
//! values for one with them ([`LanguageTraits::writes_non_finite`]), which
//! it reads with `float_of` and `string_of`. So a language's writer writes
//! strings as it is given them and never sees a mark it must take off.
//!
//! Each method is named `write_…` after the hook it calls (`write_document`
//! calls `serialize`). Code outside the languages calls only these: the
//! `one_writer_door` test fails on a call to a writer hook anywhere else.
//! A language's own code, and a hook's default that calls back into the
//! language (`ConfigLanguage::format_value(language, …)`), call the hooks:
//! their value has been through the door once already, and a second pass
//! would read a string holding the form as a number.
//!
//! [`LanguageTraits::writes_non_finite`]: super::LanguageTraits::writes_non_finite

use std::borrow::Cow;

use serde_json::Value;

use super::{ConfigLanguage, SaveTold};
use crate::editor::codec::non_finite;

impl dyn ConfigLanguage {
    /// `value` as this language's writers are given it (see the module).
    fn for_writer<'a>(&self, value: &'a Value) -> Cow<'a, Value> {
        if self.writes_non_finite() {
            Cow::Borrowed(value)
        } else {
            non_finite::plain(value)
        }
    }

    /// [`ConfigLanguage::serialize`], given `value` through the door.
    ///
    /// # Errors
    ///
    /// As [`ConfigLanguage::serialize`].
    pub fn write_document(&self, value: &Value, previous: Option<&str>) -> Result<String, String> {
        ConfigLanguage::serialize(self, &self.for_writer(value), previous)
    }

    /// [`ConfigLanguage::format_value`], given `value` through the door.
    pub fn write_value(&self, value: &Value) -> String {
        ConfigLanguage::format_value(self, &self.for_writer(value))
    }

    /// [`SaveLayout::serialize_over`](super::SaveLayout::serialize_over),
    /// else [`ConfigLanguage::serialize`], given `value` through the door.
    ///
    /// # Errors
    ///
    /// As [`ConfigLanguage::serialize`].
    pub fn write_document_over(
        &self,
        value: &Value,
        previous: Option<&str>,
        told: SaveTold<'_>,
    ) -> Result<String, String> {
        let value = self.for_writer(value);
        self.layout()
            .serialize_over(&value, previous, told)
            .unwrap_or_else(|| ConfigLanguage::serialize(self, &value, previous))
    }

    /// [`SaveLayout::one_line_body`](super::SaveLayout::one_line_body),
    /// given `value` through the door.
    pub fn write_one_line_body(&self, value: &Value, indent: &str, unit: &str) -> Option<String> {
        self.layout()
            .one_line_body(&self.for_writer(value), indent, unit)
    }

    /// [`SaveLayout::format_value_like`](super::SaveLayout::format_value_like),
    /// given `value` through the door.
    pub fn write_value_like(&self, value: &Value, written: &str) -> String {
        self.layout()
            .format_value_like(self, &self.for_writer(value), written)
    }

    /// [`SaveLayout::format_value_at`](super::SaveLayout::format_value_at),
    /// given `value` through the door.
    pub fn write_value_at(
        &self,
        value: &Value,
        source: &str,
        range: std::ops::Range<usize>,
    ) -> String {
        self.layout()
            .format_value_at(self, &self.for_writer(value), source, range)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::super::{ConfigLanguage, LanguageConfig, LanguageInfo, LanguageTraits, ParseError};
    use crate::{float_value, string_value};

    /// A language whose writers print what they are given, strings as they
    /// are; `writes_non_finite` as told.
    struct Naive(bool);

    impl ConfigLanguage for Naive {
        fn info(&self) -> LanguageInfo {
            LanguageInfo {
                label: "Naive",
                icon: "N",
                extensions: &["naive"],
            }
        }

        fn config(&self) -> LanguageConfig {
            LanguageConfig {
                traits: LanguageTraits {
                    writes_non_finite: self.0,
                    ..LanguageTraits::NONE
                },
                ..LanguageConfig::default()
            }
        }

        fn parse(&self, _source: &str) -> Result<Value, ParseError> {
            Ok(Value::Null)
        }

        fn serialize(&self, value: &Value, _previous: Option<&str>) -> Result<String, String> {
            let map = value.as_object().ok_or("a map")?;
            Ok(map
                .iter()
                .map(|(key, item)| format!("{key}={}\n", self.format_value(item)))
                .collect())
        }

        fn format_value(&self, value: &Value) -> String {
            value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_string)
        }
    }

    fn document() -> Value {
        json!({
            "rate": float_value(f64::INFINITY),
            "low": float_value(f64::NEG_INFINITY),
            "odd": float_value(f64::NAN),
            "big": u64::MAX,
            "s": string_value("\u{FDD0}inf"),
        })
    }

    #[test]
    fn a_naive_writer_is_given_plain_data_through_every_door() {
        let naive: &dyn ConfigLanguage = &Naive(false);
        let written = "rate=inf\nlow=-inf\nodd=nan\nbig=18446744073709551615\ns=\u{FDD0}inf\n";
        assert_eq!(naive.write_document(&document(), None).unwrap(), written);
        let told = super::SaveTold::default();
        assert_eq!(
            naive.write_document_over(&document(), None, told).unwrap(),
            written
        );
        let rate = float_value(f64::INFINITY);
        let forged = string_value("\u{FDD0}inf");
        assert_eq!(naive.write_value(&rate), "inf");
        assert_eq!(naive.write_value(&forged), "\u{FDD0}inf");
        // The defaults call back into the language once, not the door again.
        assert_eq!(naive.write_value_like(&forged, "x"), "\u{FDD0}inf");
        assert_eq!(naive.write_value_at(&forged, "s=x\n", 2..3), "\u{FDD0}inf");
    }

    #[test]
    fn a_language_with_infinities_is_given_the_models_values() {
        let holds: &dyn ConfigLanguage = &Naive(true);
        assert_eq!(
            holds.write_value(&float_value(f64::INFINITY)),
            "\u{FDD0}inf"
        );
        assert_eq!(
            holds.write_value(&string_value("\u{FDD0}inf")),
            "\u{FDD0}\u{FDD0}inf"
        );
    }
}
