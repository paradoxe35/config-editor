//! The chrome's looks other than its colors (fonts, spacing, borders,
//! radius): [`ThemeChrome`], beside [`ThemeColors`](super::ThemeColors).

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::tokens::{Token, TokenValues};

/// One look of the editor's chrome (the documents view's tabs, the toolbar
/// and the footer), for [`ThemeChrome`]. Each maps to a `--config-editor-*`
/// CSS variable whose default the preset stylesheet sets on the editor's
/// root; the chrome's layout (sizes, rows, hit areas) stays the editor's.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum ChromeToken {
    /// The tabs' font family (by default `ui-sans-serif, system-ui,
    /// sans-serif`); the close button's too.
    TabFontFamily,
    /// A tab's name (`12px`).
    TabFontSize,
    /// A tab's name (`normal`).
    TabFontWeight,
    /// A tab's name (`normal`).
    TabFontStyle,
    /// The active tab's name (`normal`).
    TabActiveFontWeight,
    /// A read-only document's tab name (`italic`).
    TabReadonlyFontStyle,
    /// Inside a tab (`0 6px 0 10px`; `0 6px 0 8px` at 600px wide or less).
    TabPadding,
    /// Between a tab's icon, name and close button (`6px`).
    TabGap,
    /// The border between tabs (`1px`).
    TabBorderWidth,
    /// The line over the active tab (`2px`).
    TabActiveBorderWidth,
    /// A tab's corners (`0`).
    TabRadius,
    /// The tab strip's corners (`6px 6px 0 0`).
    TabBarRadius,
    /// The close button's corners (`4px`).
    TabCloseRadius,
    /// The close button's `×` (`14px`).
    TabCloseFontSize,
    /// The toolbar's font family (`ui-sans-serif, system-ui, sans-serif`).
    ToolbarFontFamily,
    /// The toolbar's text (`12px`).
    ToolbarFontSize,
    /// The toolbar's text (`normal`).
    ToolbarFontWeight,
    /// The toolbar's text (`normal`).
    ToolbarFontStyle,
    /// Inside the toolbar (`0 14px`; `0 10px` at 600px wide or less).
    ToolbarPadding,
    /// Between the toolbar's parts (`14px`).
    ToolbarGap,
    /// The line under the toolbar (`1px`).
    ToolbarBorderWidth,
    /// The footer's font family (`ui-sans-serif, system-ui, sans-serif`).
    FooterFontFamily,
    /// The footer's text (`10px`).
    FooterFontSize,
    /// The footer's text (`normal`).
    FooterFontWeight,
    /// The footer's text (`normal`).
    FooterFontStyle,
    /// Inside the footer (`0 10px`).
    FooterPadding,
    /// Between the footer's parts (`12px`).
    FooterGap,
    /// The line over the footer (`1px`).
    FooterBorderWidth,
}

impl ChromeToken {
    /// Every token, in a stable order.
    pub const ALL: &'static [Self] = &[
        Self::TabFontFamily,
        Self::TabFontSize,
        Self::TabFontWeight,
        Self::TabFontStyle,
        Self::TabActiveFontWeight,
        Self::TabReadonlyFontStyle,
        Self::TabPadding,
        Self::TabGap,
        Self::TabBorderWidth,
        Self::TabActiveBorderWidth,
        Self::TabRadius,
        Self::TabBarRadius,
        Self::TabCloseRadius,
        Self::TabCloseFontSize,
        Self::ToolbarFontFamily,
        Self::ToolbarFontSize,
        Self::ToolbarFontWeight,
        Self::ToolbarFontStyle,
        Self::ToolbarPadding,
        Self::ToolbarGap,
        Self::ToolbarBorderWidth,
        Self::FooterFontFamily,
        Self::FooterFontSize,
        Self::FooterFontWeight,
        Self::FooterFontStyle,
        Self::FooterPadding,
        Self::FooterGap,
        Self::FooterBorderWidth,
    ];

    /// The token's name: its CSS variable without `--config-editor-`, such
    /// as `"tab-padding"`. [`ThemeChrome`] read from JSON is keyed by it.
    pub fn token(self) -> &'static str {
        Token::token(self)
    }

    /// The CSS variable this token sets, such as
    /// `"--config-editor-tab-padding"`.
    pub fn css_var(self) -> &'static str {
        match self {
            Self::TabFontFamily => "--config-editor-tab-font-family",
            Self::TabFontSize => "--config-editor-tab-font-size",
            Self::TabFontWeight => "--config-editor-tab-font-weight",
            Self::TabFontStyle => "--config-editor-tab-font-style",
            Self::TabActiveFontWeight => "--config-editor-tab-active-font-weight",
            Self::TabReadonlyFontStyle => "--config-editor-tab-readonly-font-style",
            Self::TabPadding => "--config-editor-tab-padding",
            Self::TabGap => "--config-editor-tab-gap",
            Self::TabBorderWidth => "--config-editor-tab-border-width",
            Self::TabActiveBorderWidth => "--config-editor-tab-active-border-width",
            Self::TabRadius => "--config-editor-tab-radius",
            Self::TabBarRadius => "--config-editor-tab-bar-radius",
            Self::TabCloseRadius => "--config-editor-tab-close-radius",
            Self::TabCloseFontSize => "--config-editor-tab-close-font-size",
            Self::ToolbarFontFamily => "--config-editor-toolbar-font-family",
            Self::ToolbarFontSize => "--config-editor-toolbar-font-size",
            Self::ToolbarFontWeight => "--config-editor-toolbar-font-weight",
            Self::ToolbarFontStyle => "--config-editor-toolbar-font-style",
            Self::ToolbarPadding => "--config-editor-toolbar-padding",
            Self::ToolbarGap => "--config-editor-toolbar-gap",
            Self::ToolbarBorderWidth => "--config-editor-toolbar-border-width",
            Self::FooterFontFamily => "--config-editor-footer-font-family",
            Self::FooterFontSize => "--config-editor-footer-font-size",
            Self::FooterFontWeight => "--config-editor-footer-font-weight",
            Self::FooterFontStyle => "--config-editor-footer-font-style",
            Self::FooterPadding => "--config-editor-footer-padding",
            Self::FooterGap => "--config-editor-footer-gap",
            Self::FooterBorderWidth => "--config-editor-footer-border-width",
        }
    }

    /// The CSS property a value of this token must be valid for, such as
    /// `"padding"` (the npm package checks values with `CSS.supports`).
    pub fn property(self) -> &'static str {
        match self {
            Self::TabFontFamily | Self::ToolbarFontFamily | Self::FooterFontFamily => "font-family",
            Self::TabFontSize
            | Self::TabCloseFontSize
            | Self::ToolbarFontSize
            | Self::FooterFontSize => "font-size",
            Self::TabFontWeight
            | Self::TabActiveFontWeight
            | Self::ToolbarFontWeight
            | Self::FooterFontWeight => "font-weight",
            Self::TabFontStyle
            | Self::TabReadonlyFontStyle
            | Self::ToolbarFontStyle
            | Self::FooterFontStyle => "font-style",
            Self::TabPadding | Self::ToolbarPadding | Self::FooterPadding => "padding",
            Self::TabGap | Self::ToolbarGap | Self::FooterGap => "column-gap",
            Self::TabBorderWidth | Self::ToolbarBorderWidth | Self::FooterBorderWidth => {
                "border-top-width"
            }
            Self::TabActiveBorderWidth => "height",
            Self::TabRadius | Self::TabBarRadius | Self::TabCloseRadius => "border-radius",
        }
    }
}

impl Token for ChromeToken {
    const ALL: &'static [Self] = Self::ALL;
    const KIND: &'static str = "chrome";
    const VALUE: &'static str = "a CSS value string";
    const EXPECTING: &'static str =
        "an object of token: CSS value, such as { \"tab-padding\": \"0 12px\" }";
    const LISTED: &'static str = "ThemeChromeToken (TypeScript) and ChromeToken::ALL (Rust)";

    fn css_var(self) -> &'static str {
        Self::css_var(self)
    }
}

/// The chrome's fonts, spacing, borders and radius, over the preset's: the
/// documents view's tabs, the toolbar and the footer. Its colors are
/// [`ThemeColors`](super::ThemeColors) tokens (`tab-bg`, `toolbar-bg`,
/// `footer-text`, …).
///
/// ```
/// use leptos_config_editor::{ChromeToken, ThemeChrome};
/// let roomy = ThemeChrome::new()
///     .set(ChromeToken::TabPadding, "0 12px")
///     .set(ChromeToken::FooterFontSize, "12px");
/// assert_eq!(
///     roomy.to_style(),
///     "--config-editor-tab-padding: 0 12px; --config-editor-footer-font-size: 12px;"
/// );
/// ```
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct ThemeChrome {
    values: TokenValues<ChromeToken>,
}

impl ThemeChrome {
    /// No overrides: the preset's chrome as it is.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets `token` to a CSS value (`"0 12px"`, `"600"`), replacing an
    /// earlier value for it.
    #[must_use]
    pub fn set(mut self, token: ChromeToken, value: impl Into<String>) -> Self {
        self.values.set(token, value.into());
        self
    }

    /// The overrides as inline CSS declarations. Values that could end the
    /// declaration (`;`, braces) are dropped.
    pub fn to_style(&self) -> String {
        self.values.to_style()
    }

    /// `style` and then these overrides, set through the CSSOM.
    pub(crate) fn declare(&self, style: crate::cssom::Declarations) -> crate::cssom::Declarations {
        self.values.declare(style)
    }

    /// Whether nothing is overridden.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// The overrides, in the order they were set.
    pub fn iter(&self) -> impl Iterator<Item = (ChromeToken, &str)> {
        self.values.iter()
    }
}

/// The chrome reads from and writes to an object of token: CSS value
/// (`{"tab-padding": "0 12px"}`), keyed by [`ChromeToken::token`].
impl Serialize for ThemeChrome {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.values.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ThemeChrome {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        TokenValues::deserialize(deserializer).map(|values| Self { values })
    }
}

#[cfg(test)]
mod tests {
    use super::{ChromeToken, ThemeChrome};

    /// Every token is a variable the preset stylesheet defines.
    #[test]
    fn every_chrome_token_is_a_preset_token() {
        let preset = crate::PRESET_CSS;
        for token in ChromeToken::ALL {
            assert!(
                preset.contains(&format!("{}:", token.css_var())),
                "{token:?}"
            );
            assert!(
                preset.contains(&format!("var({})", token.css_var())),
                "{token:?} is read"
            );
        }
    }

    #[test]
    fn the_chrome_reads_and_writes_tokens() {
        let chrome: ThemeChrome =
            serde_json::from_str(r#"{"tab-padding": "0 12px", "footer-gap": "4px"}"#).unwrap();
        assert_eq!(
            chrome,
            ThemeChrome::new()
                .set(ChromeToken::TabPadding, "0 12px")
                .set(ChromeToken::FooterGap, "4px")
        );
        assert_eq!(
            serde_json::to_string(&chrome).unwrap(),
            r#"{"tab-padding":"0 12px","footer-gap":"4px"}"#
        );
        let error = serde_json::from_str::<ThemeChrome>(r#"{"tab-pading": "0"}"#)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(r#"unknown chrome token "tab-pading" (did you mean "tab-padding"?)"#),
            "{error}"
        );
        let error = serde_json::from_str::<ThemeChrome>(r#"{"tab-gap": 4}"#)
            .unwrap_err()
            .to_string();
        assert!(error.contains("a CSS value string"), "{error}");
    }

    #[test]
    fn a_value_that_could_end_its_declaration_is_dropped() {
        let chrome = ThemeChrome::new()
            .set(ChromeToken::TabGap, "4px; color: red")
            .set(ChromeToken::TabPadding, " 0 2px ");
        assert_eq!(chrome.to_style(), "--config-editor-tab-padding: 0 2px;");
    }
}
