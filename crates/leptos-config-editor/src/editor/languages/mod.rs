//! Language registry: the one place a language is registered.
//!
//! Core (`language.rs`, `diagnostics.rs`, `completion.rs`, `codec.rs`,
//! `bridge.rs`, `keyboard.rs`, `commands.rs`, `editor.rs`) never names a
//! language: it reads a [`LanguageSpec`] and programs against its traits.
//! Adding a language:
//!
//! 1. write `languages/<name>.rs`: one struct implementing
//!    [`LanguageBehavior`](super::language::LanguageBehavior) and
//!    [`Codec`](super::codec::Codec), plus its `SPEC`;
//! 2. add one line to the `registry!` call below: `Name => module,`.
//!
//! Everything else (labels, icons, extensions, name parsing, conversion,
//! completion, highlighting, the component) follows from the spec.

use super::codec::{self, Codec, Conversion, ConvertError, Side};
use super::language::{LanguageBehavior, ParseError};
use serde_json::Value;

/// Everything the editor needs to know about one language, declared in the
/// language's own file as `pub(super) const SPEC`.
pub(super) struct LanguageSpec {
    /// Short UI label, such as `"YAML"`.
    pub(super) label: &'static str,
    /// Single-letter file icon.
    pub(super) icon: &'static str,
    /// File extensions, lowercase and without the dot; the first is preferred.
    pub(super) extensions: &'static [&'static str],
    /// Text layer: parsing, structure, completion, editing, highlighting.
    pub(super) behavior: &'static dyn LanguageBehavior,
    /// Serialization, for conversion.
    pub(super) codec: &'static dyn Codec,
}

/// Declares the languages: for each `Variant => module` line, the module,
/// the [`Language`] variant, its place in [`Language::ALL`], and its `spec`
/// arm, so registering a language is that one line.
macro_rules! registry {
    ($($(#[$attribute:meta])* $variant:ident => $module:ident,)+) => {
        $(mod $module;)+

        /// A document language.
        ///
        /// Parses from a name or file extension, case-insensitively:
        ///
        /// ```
        /// use leptos_config_editor::Language;
        /// assert_eq!("yml".parse::<Language>(), Ok(Language::Yaml));
        /// assert_eq!(Language::from_path("infra/main.tf"), Some(Language::Hcl));
        /// assert_eq!(Language::Toml.to_string(), "TOML");
        /// ```
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
        #[non_exhaustive]
        pub enum Language {
            $($(#[$attribute])* $variant,)+
        }

        impl Language {
            /// Every supported language, in registration order (for pickers).
            pub const ALL: &'static [Language] = &[$(Language::$variant),+];

            /// This language's registry entry.
            fn spec(self) -> &'static LanguageSpec {
                match self {
                    $(Language::$variant => &$module::SPEC,)+
                }
            }
        }
    };
}

registry! {
    /// YAML (the default).
    #[default]
    Yaml => yaml,
    /// JSON.
    Json => json,
    /// TOML.
    Toml => toml,
    /// HCL (HashiCorp Configuration Language, as used by Terraform).
    Hcl => hcl,
    /// XML.
    Xml => xml,
}

impl Language {
    /// Short UI label: `"YAML"`, `"JSON"`, `"TOML"`, `"HCL"`, or `"XML"`.
    pub fn label(self) -> &'static str {
        self.spec().label
    }

    /// Single-letter file icon for toolbars.
    pub fn file_icon(self) -> &'static str {
        self.spec().icon
    }

    /// File extensions, lowercase and without the dot; the first is the
    /// preferred one.
    pub fn extensions(self) -> &'static [&'static str] {
        self.spec().extensions
    }

    /// The language of a file, from its extension.
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
        format!("untitled.{}", self.extensions()[0])
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

    pub(super) fn behavior(self) -> &'static dyn LanguageBehavior {
        self.spec().behavior
    }

    pub(super) fn codec(self) -> &'static dyn Codec {
        self.spec().codec
    }

    pub(super) fn side(self) -> Side<'static> {
        Side {
            behavior: self.behavior(),
            codec: self.codec(),
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

/// A name that is not a supported [`Language`].
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

    /// A language label or file extension, ignoring case and a leading dot.
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
    //! Registry invariants. Generic over `Language::ALL`: registering or
    //! removing a language never requires editing these tests.

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

    #[test]
    fn the_default_language_is_registered() {
        assert!(Language::ALL.contains(&Language::default()));
    }
}
