//! Language registry: the built-in languages, each behind its Cargo feature.
//!
//! Core code never names a language: it holds a [`Language`] and calls the
//! [`ConfigLanguage`] trait behind it. Adding a built-in language:
//!
//! 1. write `languages/<name>.rs` with one struct implementing
//!    [`ConfigLanguage`];
//! 2. add its feature to `Cargo.toml` and one line to the `registry!` call
//!    below.
//!
//! External crates implement [`ConfigLanguage`] the same way and wrap it with
//! [`Language::new`].

use super::codec::{self, Conversion, ConvertError};
use super::language::{ConfigLanguage, LanguageInfo, ParseError};
use super::paths::{self, FieldPath, TextPosition, TextRange};
use serde_json::Value;

#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod accept_tests;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod completion_matrix;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod editing_matrix;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml",
    feature = "folding"
))]
mod fold_matrix;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod nested_tests;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod non_finite_matrix;
#[cfg(all(
    test,
    feature = "schema-validation",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod qa_regressions;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod schema_help_matrix;

/// A document language: a built-in one ([`Language::YAML`], …) or any
/// [`ConfigLanguage`] implementation wrapped with [`Language::new`].
///
/// A small `Copy` handle; two handles are equal when their labels are.
/// Built-in languages parse from a name or file extension:
///
/// ```
/// # #[cfg(all(feature = "hcl", feature = "toml", feature = "yaml"))] {
/// use config_editor_leptos::Language;
/// assert_eq!("yml".parse::<Language>(), Ok(Language::YAML));
/// assert_eq!(Language::from_path("infra/main.tf"), Some(Language::HCL));
/// assert_eq!(Language::TOML.to_string(), "TOML");
/// # }
/// ```
#[derive(Clone, Copy)]
pub struct Language(&'static dyn ConfigLanguage);

/// Declares the built-in languages: per line, the feature that enables it,
/// its constant, and its implementation.
// Changed strings keep the host's escapes.
#[cfg(any(feature = "json", feature = "yaml", feature = "toml", feature = "hcl"))]
mod escapes;

macro_rules! registry {
    ($($(#[doc = $doc:literal])* $feature:literal => $constant:ident => $module:ident::$implementation:ident,)+) => {
        $(#[cfg(feature = $feature)] mod $module;)+

        impl Language {
            $(
                $(#[doc = $doc])*
                #[cfg(feature = $feature)]
                pub const $constant: Language = Language(&$module::$implementation);
            )+

            /// The built-in languages this build enables, in a stable order
            /// (for pickers).
            pub const ALL: &'static [Language] = &[$(#[cfg(feature = $feature)] Language::$constant,)+];
        }
    };
}

registry! {
    /// YAML (feature `yaml`).
    "yaml" => YAML => yaml::YamlBehavior,
    /// JSON (feature `json`).
    "json" => JSON => json::JsonBehavior,
    /// TOML (feature `toml`).
    "toml" => TOML => toml::TomlBehavior,
    /// HCL, the HashiCorp Configuration Language used by Terraform (feature
    /// `hcl`).
    "hcl" => HCL => hcl::HclBehavior,
    /// XML (feature `xml`).
    "xml" => XML => xml::XmlBehavior,
}

impl Language {
    /// Wraps a language implementation, built-in or external.
    ///
    /// ```
    /// use config_editor_leptos::Language;
    /// use config_editor_leptos::language::{ConfigLanguage, LanguageInfo, ParseError};
    ///
    /// struct Lines;
    /// impl ConfigLanguage for Lines {
    ///     fn info(&self) -> LanguageInfo {
    ///         LanguageInfo { label: "Lines", icon: "L", extensions: &["lines"] }
    ///     }
    ///     fn parse(&self, source: &str) -> Result<serde_json::Value, ParseError> {
    ///         Ok(source.lines().collect::<Vec<_>>().into())
    ///     }
    /// }
    ///
    /// static LINES: Lines = Lines;
    /// let language = Language::new(&LINES);
    /// assert_eq!(language.label(), "Lines");
    /// assert_eq!(language.parse("a\nb").unwrap()[1], "b");
    /// ```
    pub const fn new(language: &'static dyn ConfigLanguage) -> Self {
        Language(language)
    }

    /// The language's label, icon, and extensions.
    pub fn info(self) -> LanguageInfo {
        self.0.info()
    }

    /// Short UI label, such as `"YAML"`.
    pub fn label(self) -> &'static str {
        self.info().label
    }

    /// Single-character toolbar icon.
    pub fn file_icon(self) -> &'static str {
        self.info().icon
    }

    /// File extensions, lowercase and without the dot; the first is preferred.
    pub fn extensions(self) -> &'static [&'static str] {
        self.info().extensions
    }

    /// The built-in language of a file, from its extension.
    pub fn from_path(path: &str) -> Option<Language> {
        let (_, extension) = path.rsplit_once('.')?;
        Language::ALL.iter().copied().find(|language| {
            language
                .extensions()
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(extension))
        })
    }

    /// File name the built-in toolbar shows when the host gives none.
    pub fn default_file_name(self) -> String {
        match self.extensions().first() {
            Some(extension) => format!("untitled.{extension}"),
            None => "untitled".to_string(),
        }
    }

    /// Parses a document into the shared data model (a JSON value), the same
    /// model schema validation sees.
    ///
    /// # Errors
    ///
    /// The first syntax error, with its position when the language reports
    /// one.
    ///
    /// ```
    /// # #[cfg(feature = "toml")] {
    /// use config_editor_leptos::Language;
    /// let value = Language::TOML.parse("listen = 7835\n").unwrap();
    /// assert_eq!(value["listen"], 7835);
    /// # }
    /// ```
    pub fn parse(self, source: &str) -> Result<Value, ParseError> {
        self.0.parse(source)
    }

    /// Renders a data model value as a document in this language, only when
    /// the document reads back as exactly that value (the same check as a
    /// conversion's `losses`).
    ///
    /// # Errors
    ///
    /// [`ConvertError::Unsupported`] when the language cannot express the
    /// value (TOML has no `null`), or would read it back differently (XML
    /// has no empty or nested lists and reads a one-item list as its item;
    /// JSON, HCL and XML have no infinity or NaN).
    ///
    /// ```
    /// # #[cfg(feature = "hcl")] {
    /// use config_editor_leptos::Language;
    /// let value = serde_json::json!({"web": {"http": 80}});
    /// assert_eq!(Language::HCL.serialize(&value).unwrap(), "web {\n  http = 80\n}\n");
    /// # }
    /// ```
    pub fn serialize(self, value: &Value) -> Result<String, ConvertError> {
        codec::serialize_exact(self.0, value)
    }

    /// Converts a document from this language to `target`. The result lists
    /// the JSON pointers `target` could not carry exactly (`losses`);
    /// comments do not survive a conversion.
    ///
    /// # Errors
    ///
    /// [`ConvertError::Parse`] when `source` does not parse, and
    /// [`ConvertError::Unsupported`] when `target` cannot express it.
    ///
    /// ```
    /// # #[cfg(all(feature = "hcl", feature = "yaml"))] {
    /// use config_editor_leptos::Language;
    /// let hcl = Language::YAML.convert("web:\n  http: 80\n", Language::HCL).unwrap();
    /// assert_eq!(hcl.text, "web {\n  http = 80\n}\n");
    /// assert!(hcl.losses.is_empty());
    /// # }
    /// ```
    pub fn convert(self, source: &str, target: Language) -> Result<Conversion, ConvertError> {
        codec::convert(source, self.0, target.0, None, None)
    }

    /// Where the field at `path` is written in `source`: its key and value
    /// when they share a line (`listen: 7835`), the key alone when the value
    /// is a block below it (an object or list over several lines), the value
    /// alone for an array item; the root is the whole text. `None` when the
    /// document has no such field.
    ///
    /// ```
    /// # #[cfg(all(feature = "yaml", feature = "toml"))] {
    /// use config_editor_leptos::{Language, TextPosition};
    /// let text = "web:\n  http: 80\n";
    /// let range = Language::YAML.range_of(text, "/web/http").unwrap();
    /// assert_eq!(range.start, TextPosition::new(2, 3));
    /// assert_eq!(range.slice(text), "http: 80");
    /// let toml = "[web]\nhttp = 80\n";
    /// assert_eq!(Language::TOML.range_of(toml, "/web/http").unwrap().slice(toml), "http = 80");
    /// # }
    /// ```
    pub fn range_of(self, source: &str, path: impl Into<FieldPath>) -> Option<TextRange> {
        paths::field_range(self.0, source, &path.into().to_pointer())
    }

    /// The line ranges of `source` that fold (see
    /// [`ConfigLanguage::fold_ranges`]): 0-based, the first line staying
    /// visible. Needs the `folding` feature.
    ///
    /// ```
    /// # #[cfg(all(feature = "json", feature = "folding"))] {
    /// use config_editor_leptos::{Language, language::FoldRange};
    /// let text = "{\n  \"web\": {\n    \"http\": 80\n  }\n}";
    /// assert_eq!(
    ///     Language::JSON.fold_ranges(text),
    ///     [FoldRange::new(0, 3), FoldRange::new(1, 2)]
    /// );
    /// # }
    /// ```
    #[cfg(feature = "folding")]
    pub fn fold_ranges(self, source: &str) -> Vec<super::language::FoldRange> {
        self.0.fold_ranges(source)
    }

    /// The path of what `position` is on in `source`: the key under or just
    /// before it, else the value or container holding it (the root at the
    /// top level). `None` past the end of the text.
    ///
    /// ```
    /// # #[cfg(feature = "yaml")] {
    /// use config_editor_leptos::{FieldPath, Language, TextPosition};
    /// let text = "web:\n  http: 80\n";
    /// let path = Language::YAML.path_at(text, TextPosition::new(2, 10)).unwrap();
    /// assert_eq!(path, FieldPath::from("/web/http"));
    /// # }
    /// ```
    pub fn path_at(self, source: &str, position: TextPosition) -> Option<FieldPath> {
        let lines = source.split('\n').count();
        if position.line > lines {
            return None;
        }
        paths::path_at(self.0, source, position.to_offset(source))
    }

    pub(super) fn behavior(self) -> &'static dyn ConfigLanguage {
        self.0
    }
}

/// The first enabled built-in language (YAML with default features).
///
/// # Panics
///
/// When no built-in language feature is enabled; pass an explicit
/// [`Language::new`] language instead.
impl Default for Language {
    fn default() -> Self {
        *Language::ALL
            .first()
            .expect("no built-in language is enabled; use Language::new")
    }
}

impl PartialEq for Language {
    fn eq(&self, other: &Self) -> bool {
        self.label() == other.label()
    }
}

impl Eq for Language {}

impl std::hash::Hash for Language {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.label().hash(state);
    }
}

impl std::fmt::Debug for Language {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("Language")
            .field(&self.label())
            .finish()
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

/// A name that is not a built-in [`Language`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownLanguage(pub String);

impl std::fmt::Display for UnknownLanguage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "unknown config language \"{}\"", self.0)
    }
}

impl std::error::Error for UnknownLanguage {}

impl std::str::FromStr for Language {
    type Err = UnknownLanguage;

    /// A built-in language's label or file extension, ignoring case and a
    /// leading dot.
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        let wanted = name.trim().trim_start_matches('.');
        Language::ALL
            .iter()
            .copied()
            .find(|language| {
                language.label().eq_ignore_ascii_case(wanted)
                    || language
                        .extensions()
                        .iter()
                        .any(|extension| extension.eq_ignore_ascii_case(wanted))
            })
            .ok_or_else(|| UnknownLanguage(name.to_string()))
    }
}

#[cfg(test)]
mod tests {
    //! Registry invariants, generic over `Language::ALL`.

    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_language_has_a_complete_registry_entry() {
        for &language in Language::ALL {
            assert!(!language.label().is_empty(), "{language:?}");
            assert_eq!(language.file_icon().chars().count(), 1, "{language:?}");
            assert!(!language.extensions().is_empty(), "{language:?}");
            for extension in language.extensions() {
                assert_eq!(
                    *extension,
                    extension.to_ascii_lowercase(),
                    "{language:?} extensions are lowercase"
                );
                assert!(
                    !extension.starts_with('.'),
                    "{language:?} extensions have no dot"
                );
            }
        }
    }

    #[test]
    fn labels_and_extensions_identify_one_language_each() {
        let labels: HashSet<&str> = Language::ALL
            .iter()
            .map(|language| language.label())
            .collect();
        assert_eq!(labels.len(), Language::ALL.len(), "labels are unique");
        let extensions: Vec<&str> = Language::ALL
            .iter()
            .flat_map(|language| language.extensions().iter().copied())
            .collect();
        let unique: HashSet<&str> = extensions.iter().copied().collect();
        assert_eq!(unique.len(), extensions.len(), "extensions are unique");
    }

    #[test]
    fn every_name_a_language_answers_to_parses_back_to_it() {
        for &language in Language::ALL {
            assert_eq!(language.label().parse::<Language>(), Ok(language));
            assert_eq!(language.to_string(), language.label());
            for extension in language.extensions() {
                assert_eq!(extension.parse::<Language>(), Ok(language));
                assert_eq!(
                    format!(".{}", extension.to_ascii_uppercase()).parse::<Language>(),
                    Ok(language)
                );
                assert_eq!(
                    Language::from_path(&format!("dir/config.{extension}")),
                    Some(language)
                );
            }
        }
    }

    #[test]
    fn default_file_names_carry_the_preferred_extension() {
        for &language in Language::ALL {
            assert_eq!(
                language.default_file_name(),
                format!("untitled.{}", language.extensions()[0])
            );
        }
    }

    #[test]
    fn unknown_names_and_paths_are_rejected() {
        assert_eq!(Language::from_path("README"), None);
        assert_eq!(Language::from_path("notes.unknown-extension"), None);
        assert_eq!(
            "unknown-language".parse::<Language>(),
            Err(UnknownLanguage("unknown-language".to_string()))
        );
    }

    #[cfg(any(
        feature = "yaml",
        feature = "json",
        feature = "toml",
        feature = "hcl",
        feature = "xml"
    ))]
    #[test]
    fn the_default_language_is_registered() {
        assert!(Language::ALL.contains(&Language::default()));
    }
}
