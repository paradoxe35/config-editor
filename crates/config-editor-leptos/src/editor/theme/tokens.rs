//! What [`ThemeColors`](super::ThemeColors) and [`ThemeChrome`](super::ThemeChrome)
//! share: CSS values keyed by a token, each a `--config-editor-*` variable
//! the editor sets on its root, read from and written to an object of
//! token: value.

use serde::de::{MapAccess, Visitor};
use serde::{Deserializer, Serializer};
use std::fmt;
use std::marker::PhantomData;

/// A kind of token: one CSS variable per value.
pub(crate) trait Token: Copy + Eq + fmt::Debug + 'static {
    /// Every token, in a stable order.
    const ALL: &'static [Self];
    /// What the tokens are called in an error: `"color"`.
    const KIND: &'static str;
    /// What a value is, in an error: `"a CSS color string"`.
    const VALUE: &'static str;
    /// What the object is, in an error.
    const EXPECTING: &'static str;
    /// Where the tokens are listed, in an error.
    const LISTED: &'static str;

    /// The CSS variable this token sets, such as `"--config-editor-key"`.
    fn css_var(self) -> &'static str;

    /// The token's name: its variable without `--config-editor-`.
    fn token(self) -> &'static str {
        self.css_var()
            .strip_prefix("--config-editor-")
            .unwrap_or_default()
    }
}

/// Values by token, in the order they were set.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct TokenValues<T>(Vec<(T, String)>);

impl<T> Default for TokenValues<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<T: Token> TokenValues<T> {
    /// Sets `token` to `value`, replacing an earlier value for it.
    pub(crate) fn set(&mut self, token: T, value: String) {
        match self.0.iter_mut().find(|(existing, _)| *existing == token) {
            Some((_, current)) => *current = value,
            None => self.0.push((token, value)),
        }
    }

    /// The values that cannot end their declaration (`;`, braces), trimmed.
    fn kept(&self) -> impl Iterator<Item = (T, &str)> {
        self.0
            .iter()
            .filter(|(_, value)| !value.contains([';', '{', '}']))
            .map(|(token, value)| (*token, value.trim()))
    }

    /// The values as inline CSS declarations.
    pub(crate) fn to_style(&self) -> String {
        self.kept()
            .map(|(token, value)| format!("{}: {value};", token.css_var()))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The values as declarations set through the CSSOM (which a strict
    /// CSP allows), with the same values [`Self::to_style`] keeps, after
    /// `style`'s.
    pub(crate) fn declare(&self, style: crate::cssom::Declarations) -> crate::cssom::Declarations {
        self.kept().fold(style, |style, (token, value)| {
            style.set(token.css_var(), value)
        })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (T, &str)> {
        self.0.iter().map(|(token, value)| (*token, value.as_str()))
    }

    pub(crate) fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.iter().map(|(token, value)| (token.token(), value)))
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Self, D::Error> {
        deserializer.deserialize_map(MapVisitor(PhantomData))
    }
}

struct MapVisitor<T>(PhantomData<T>);

impl<'de, T: Token> Visitor<'de> for MapVisitor<T> {
    type Value = TokenValues<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(T::EXPECTING)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut values = TokenValues::default();
        while let Some(name) = map.next_key::<String>()? {
            let token = T::ALL
                .iter()
                .copied()
                .find(|token| token.token() == name)
                .ok_or_else(|| serde::de::Error::custom(unknown_token::<T>(&name)))?;
            let value = map.next_value::<CssValue<T>>()?;
            values.set(token, value.0);
        }
        Ok(values)
    }
}

/// The error for a token that is not one: the nearest token, and where
/// they are all listed.
fn unknown_token<T: Token>(name: &str) -> String {
    let nearest = T::ALL
        .iter()
        .map(|token| (distance(name, token.token()), token.token()))
        .min()
        .filter(|(distance, _)| *distance <= 2)
        .map(|(_, nearest)| format!(" (did you mean \"{nearest}\"?)"))
        .unwrap_or_default();
    format!(
        "unknown {} token \"{name}\"{nearest}; the tokens are listed by {}",
        T::KIND,
        T::LISTED
    )
}

/// Characters to insert, delete or replace to turn `one` into `other`.
fn distance(one: &str, other: &str) -> usize {
    let other: Vec<char> = other.chars().collect();
    let mut row: Vec<usize> = (0..=other.len()).collect();
    for (index, character) in one.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = index + 1;
        for (at, wanted) in other.iter().enumerate() {
            let next = (diagonal + usize::from(character != *wanted))
                .min(row[at] + 1)
                .min(row[at + 1] + 1);
            diagonal = row[at + 1];
            row[at + 1] = next;
        }
    }
    row[other.len()]
}

/// A value, which must be a string.
struct CssValue<T>(String, PhantomData<T>);

impl<'de, T: Token> serde::Deserialize<'de> for CssValue<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StringVisitor<T>(PhantomData<T>);
        impl<T: Token> Visitor<'_> for StringVisitor<T> {
            type Value = CssValue<T>;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(T::VALUE)
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<CssValue<T>, E> {
                Ok(CssValue(value.to_string(), PhantomData))
            }
        }
        deserializer.deserialize_str(StringVisitor(PhantomData))
    }
}
