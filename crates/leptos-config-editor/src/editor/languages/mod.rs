//! Language registry: the [`Language`] selector plus each variant's behavior.
//!
//! This is the only place (besides the `<ConfigEditor language=…>` call site)
//! that names document languages. Core (`language.rs`, `diagnostics.rs`,
//! `completion.rs`, `keyboard.rs`, `commands.rs`, `editor.rs`) never matches on
//! variants and never imports a language file — it calls `language.behavior()`
//! and programs against the [`LanguageBehavior`](super::language::LanguageBehavior)
//! trait. Adding a language means one new file here plus one line in the
//! `match` below — no core changes.

use super::codec::{self, Codec, Conversion, ConvertError, Side};
use super::language::{LanguageBehavior, ParseError};
use serde_json::Value;

mod hcl;
mod json;
mod toml;
mod xml;
mod yaml;
use hcl::HclBehavior;
use json::JsonBehavior;
use toml::TomlBehavior;
use xml::XmlBehavior;
use yaml::YamlBehavior;

static YAML_BEHAVIOR: YamlBehavior = YamlBehavior;
static JSON_BEHAVIOR: JsonBehavior = JsonBehavior;
static TOML_BEHAVIOR: TomlBehavior = TomlBehavior;
static HCL_BEHAVIOR: HclBehavior = HclBehavior;
static XML_BEHAVIOR: XmlBehavior = XmlBehavior;

/// Document language edited by the component.
///
/// A plain selector: no syntax lives here. Selects the text layer (parsing,
/// completion context, inserts, hover lookup, highlighting, newline/indent,
/// comments). Everything else — history, diffs, completion ranking, schema
/// navigation, popups — is language-independent and stays shared.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Language {
    /// YAML documents (default; preserves the original editor behavior).
    #[default]
    Yaml,
    /// JSON documents.
    Json,
    /// TOML documents.
    Toml,
    /// HCL documents (HashiCorp Configuration Language, as used by Terraform).
    Hcl,
    /// XML documents.
    Xml,
}

impl Language {
    /// Short UI label: `"YAML"`, `"JSON"`, `"TOML"`, `"HCL"`, or `"XML"`. Toolbar,
    /// aria labels, and diagnostic messages derive from this so no language name
    /// is hard-coded in markup.
    pub fn label(self) -> &'static str {
        match self {
            Language::Yaml => "YAML",
            Language::Json => "JSON",
            Language::Toml => "TOML",
            Language::Hcl => "HCL",
            Language::Xml => "XML",
        }
    }

    /// Single-letter file icon for the built-in toolbar.
    pub fn file_icon(self) -> &'static str {
        match self {
            Language::Yaml => "Y",
            Language::Json => "J",
            Language::Toml => "T",
            Language::Hcl => "H",
            Language::Xml => "X",
        }
    }

    /// Default file name used by the built-in toolbar when the host provides
    /// none.
    pub fn default_file_name(self) -> &'static str {
        match self {
            Language::Yaml => "untitled.yaml",
            Language::Json => "untitled.json",
            Language::Toml => "untitled.toml",
            Language::Hcl => "untitled.hcl",
            Language::Xml => "untitled.xml",
        }
    }

    /// The behavior implementing this language's text layer. Returned as a
    /// `&'static` reference so call sites and closures share it freely.
    pub(super) fn behavior(self) -> &'static dyn LanguageBehavior {
        match self {
            Language::Yaml => &YAML_BEHAVIOR,
            Language::Json => &JSON_BEHAVIOR,
            Language::Toml => &TOML_BEHAVIOR,
            Language::Hcl => &HCL_BEHAVIOR,
            Language::Xml => &XML_BEHAVIOR,
        }
    }

    /// This language's serializer: a separate registry from [`Self::behavior`]
    /// so nothing reaches a serializer unless conversion is used.
    pub(super) fn codec(self) -> &'static dyn Codec {
        match self {
            Language::Yaml => &YAML_BEHAVIOR,
            Language::Json => &JSON_BEHAVIOR,
            Language::Toml => &TOML_BEHAVIOR,
            Language::Hcl => &HCL_BEHAVIOR,
            Language::Xml => &XML_BEHAVIOR,
        }
    }

    pub(super) fn side(self) -> Side<'static> {
        Side {
            behavior: self.behavior(),
            codec: self.codec(),
        }
    }

    /// Parses a document into the shared data model (a JSON value), the same
    /// model schema validation sees.
    ///
    /// ```
    /// use leptos_config_editor::Language;
    /// let value = Language::Toml.parse("listen = 7835\n").unwrap();
    /// assert_eq!(value["listen"], 7835);
    /// ```
    pub fn parse(self, source: &str) -> Result<Value, ParseError> {
        self.behavior().parse(source)
    }

    /// Renders a data model value as a document in this language.
    ///
    /// ```
    /// use leptos_config_editor::Language;
    /// let value = serde_json::json!({"web": {"http": 80}});
    /// assert_eq!(Language::Hcl.serialize(&value).unwrap(), "web {\n  http = 80\n}\n");
    /// ```
    pub fn serialize(self, value: &Value) -> Result<String, ConvertError> {
        codec::serialize(self.codec(), value, None)
    }

    /// Converts a document from this language to `target`. The result lists
    /// the JSON pointers `target` could not carry exactly (`losses`); comments
    /// do not survive a conversion.
    ///
    /// ```
    /// use leptos_config_editor::Language;
    /// let hcl = Language::Yaml.convert("web:\n  http: 80\n", Language::Hcl).unwrap();
    /// assert_eq!(hcl.text, "web {\n  http = 80\n}\n");
    /// assert!(hcl.losses.is_empty());
    /// ```
    pub fn convert(self, source: &str, target: Language) -> Result<Conversion, ConvertError> {
        codec::convert(source, self.behavior(), &target.side(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_language_is_yaml() {
        assert_eq!(Language::default(), Language::Yaml);
    }

    #[test]
    fn language_labels_and_icons_are_stable() {
        assert_eq!(Language::Yaml.label(), "YAML");
        assert_eq!(Language::Json.label(), "JSON");
        assert_eq!(Language::Toml.label(), "TOML");
        assert_eq!(Language::Hcl.label(), "HCL");
        assert_eq!(Language::Xml.label(), "XML");
        assert_eq!(Language::Yaml.file_icon(), "Y");
        assert_eq!(Language::Json.file_icon(), "J");
        assert_eq!(Language::Toml.file_icon(), "T");
        assert_eq!(Language::Hcl.file_icon(), "H");
        assert_eq!(Language::Xml.file_icon(), "X");
    }

    #[test]
    fn default_file_names_carry_the_language_extension() {
        assert_eq!(Language::Yaml.default_file_name(), "untitled.yaml");
        assert_eq!(Language::Json.default_file_name(), "untitled.json");
        assert_eq!(Language::Toml.default_file_name(), "untitled.toml");
        assert_eq!(Language::Hcl.default_file_name(), "untitled.hcl");
        assert_eq!(Language::Xml.default_file_name(), "untitled.xml");
    }
}
