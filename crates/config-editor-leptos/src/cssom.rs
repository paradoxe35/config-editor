//! Inline styles written through the CSSOM (`style.setProperty`), never as a
//! `style` attribute: a strict Content-Security-Policy (`style-src` without
//! `'unsafe-inline'`) refuses `setAttribute("style", …)`, which is how
//! Leptos writes `style=<String>`, but allows the CSSOM.
//!
//! In `view!`, every `style=` goes through [`cssom`], which takes
//! [`Declarations`] (or an `Option`, or a closure returning either) only; the
//! `no_style_attributes` test keeps it that way. A single property can also
//! use `style:<name>=`, which Leptos writes through the CSSOM too.

use std::borrow::Cow;
use std::fmt;

use leptos::tachys::html::style::IntoStyle;
use leptos::tachys::renderer::Rndr;
use leptos::tachys::renderer::types::{CssStyleDeclaration, Element};

/// CSS declarations (`name: value`), set one by one with `setProperty`.
///
/// Rebuilt with other declarations, it removes the properties no longer
/// given and sets the changed ones; properties it never set (`style:<name>=`
/// on the same element) are left alone.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Declarations(Vec<(Cow<'static, str>, String)>);

impl Declarations {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Adds `name: value` (a later value for the same name replaces it).
    #[must_use]
    pub(crate) fn set(
        mut self,
        name: impl Into<Cow<'static, str>>,
        value: impl Into<String>,
    ) -> Self {
        let name = name.into();
        let value = value.into();
        match self.0.iter_mut().find(|(existing, _)| *existing == name) {
            Some((_, current)) => *current = value,
            None => self.0.push((name, value)),
        }
        self
    }

    /// The value given for `name`.
    #[cfg(test)]
    pub(crate) fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(existing, _)| existing == name)
            .map(|(_, value)| value.as_str())
    }

    fn apply(&self, style: &CssStyleDeclaration, before: &Self) {
        for (name, _) in &before.0 {
            if !self.0.iter().any(|(kept, _)| kept == name) {
                Rndr::remove_css_property(style, name);
            }
        }
        for (name, value) in &self.0 {
            let unchanged = before
                .0
                .iter()
                .any(|(old, old_value)| old == name && old_value == value);
            if !unchanged {
                Rndr::set_css_property(style, name, value);
            }
        }
    }
}

/// `name:value;` per declaration, as an inline style would read.
impl fmt::Display for Declarations {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for (name, value) in &self.0 {
            if !first {
                formatter.write_str(";")?;
            }
            first = false;
            write!(formatter, "{name}:{value}")?;
        }
        Ok(())
    }
}

impl IntoStyle for Declarations {
    type AsyncOutput = Self;
    type State = (CssStyleDeclaration, Self);
    type Cloneable = Self;
    type CloneableOwned = Self;

    fn to_html(self, style: &mut String) {
        for (name, value) in &self.0 {
            style.push_str(name);
            style.push(':');
            style.push_str(value);
            style.push(';');
        }
    }

    fn hydrate<const FROM_SERVER: bool>(self, el: &Element) -> Self::State {
        self.build(el)
    }

    fn build(self, el: &Element) -> Self::State {
        let style = Rndr::style(el);
        self.apply(&style, &Self::default());
        (style, self)
    }

    fn rebuild(self, state: &mut Self::State) {
        self.apply(&state.0, &state.1);
        state.1 = self;
    }

    fn into_cloneable(self) -> Self::Cloneable {
        self
    }

    fn into_cloneable_owned(self) -> Self::CloneableOwned {
        self
    }

    fn dry_resolve(&mut self) {}

    async fn resolve(self) -> Self::AsyncOutput {
        self
    }

    fn reset(state: &mut Self::State) {
        Self::default().apply(&state.0, &state.1);
        state.1 = Self::default();
    }
}

/// What [`cssom`] takes: [`Declarations`], an `Option` of them, or a
/// closure returning either.
pub(crate) trait Cssom {}

impl Cssom for Declarations {}
impl Cssom for Option<Declarations> {}
impl Cssom for leptos::prelude::Signal<Declarations> {}
impl<F, T> Cssom for F
where
    F: FnMut() -> T,
    T: Cssom,
{
}

/// The only way into `style=` in `view!`: declarations written through the
/// CSSOM, so the style holds under a strict CSP.
pub(crate) fn cssom<S: Cssom + IntoStyle>(style: S) -> S {
    style
}

#[cfg(test)]
mod tests {
    use super::Declarations;

    #[test]
    fn displays_as_an_inline_style_and_replaces_repeated_names() {
        let style = Declarations::new()
            .set("left", "1px")
            .set("top", "2px")
            .set("left", "3px");
        assert_eq!(style.to_string(), "left:3px;top:2px");
        assert_eq!(style.get("left"), Some("3px"));
    }
}
