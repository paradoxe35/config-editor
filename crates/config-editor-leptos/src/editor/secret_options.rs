//! [`SecretOptions`], the type of the `secrets` prop: present in every
//! build, as the prop is; the `protect` feature acts on it.

use leptos::prelude::Callback;

/// The default placeholder pattern.
const SECRET_PATTERN: &str = "«secret [0-9a-f]+»";

/// The default reference forms offered in a secret field.
const REFERENCE_PREFIXES: [&str; 2] = ["${", "file:"];

/// How the editor treats secret placeholders, for the `secrets` prop.
///
/// ```
/// use config_editor_leptos::SecretOptions;
/// let options = SecretOptions::default()
///     .with_placeholder(r"<redacted:[a-z0-9]+>")
///     .with_hint("Leave as is to keep the stored secret")
///     .with_reference_prefixes(["${", "vault:"]);
/// assert_eq!(options.reference_prefixes, vec!["${", "vault:"]);
/// ```
#[derive(Clone)]
#[non_exhaustive]
pub struct SecretOptions {
    /// A regular expression matching placeholder tokens (default
    /// `«secret [0-9a-f]+»`). A token is real only when the host's own
    /// value holds it: typed or pasted look-alikes stay plain text.
    pub placeholder: String,
    /// Decides which matched tokens are real placeholders, after the
    /// pattern (and before the check that the host's value holds them).
    pub is_placeholder: Option<Callback<String, bool>>,
    /// The hover text of a placeholder; unset, the `secretHint` text of
    /// the `strings` setting.
    pub hint: Option<String>,
    /// The reference forms offered in a secret field's value (`${` opens
    /// `${VAR}`, `file:` writes `file:`).
    pub reference_prefixes: Vec<String>,
}

impl Default for SecretOptions {
    fn default() -> Self {
        SecretOptions {
            placeholder: SECRET_PATTERN.to_string(),
            is_placeholder: None,
            hint: None,
            reference_prefixes: REFERENCE_PREFIXES
                .iter()
                .map(|prefix| prefix.to_string())
                .collect(),
        }
    }
}

impl std::fmt::Debug for SecretOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SecretOptions")
            .field("placeholder", &self.placeholder)
            .field("is_placeholder", &self.is_placeholder.is_some())
            .field("hint", &self.hint)
            .field("reference_prefixes", &self.reference_prefixes)
            .finish()
    }
}

impl SecretOptions {
    /// The placeholder pattern (a regular expression).
    #[must_use]
    pub fn with_placeholder(mut self, pattern: impl Into<String>) -> Self {
        self.placeholder = pattern.into();
        self
    }

    /// A host check deciding which matched tokens are real placeholders.
    #[must_use]
    pub fn with_matcher(
        mut self,
        is_placeholder: impl Fn(String) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.is_placeholder = Some(Callback::new(is_placeholder));
        self
    }

    /// The hover text of a placeholder.
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// The reference forms offered in a secret field.
    #[must_use]
    pub fn with_reference_prefixes<T: Into<String>>(
        mut self,
        prefixes: impl IntoIterator<Item = T>,
    ) -> Self {
        self.reference_prefixes = prefixes.into_iter().map(Into::into).collect();
        self
    }
}
