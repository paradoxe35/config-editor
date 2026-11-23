use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

mod chrome;
mod tokens;

pub use chrome::{ChromeToken, ThemeChrome};
use tokens::{Token, TokenValues};

/// A bundled color theme.
///
/// All themes share the same layout; only the palette changes.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum EditorTheme {
    #[default]
    /// VS Code Dark+ (the default).
    VsCodeDark,
    /// VS Code Light+.
    VsCodeLight,
    /// Obsidian dark.
    Obsidian,
    /// Obsidian light.
    ObsidianLight,
    /// Dracula.
    Dracula,
    /// Nord.
    Nord,
    /// Atom One Dark.
    AtomOneDark,
    /// GitHub light.
    GitHubLight,
    /// GitHub dark.
    GitHubDark,
    /// Solarized light.
    SolarizedLight,
    /// An original palette inspired by Vira.
    ViraInspired,
}

impl EditorTheme {
    /// All bundled themes, in a stable order for pickers.
    pub const ALL: &'static [Self] = &[
        Self::VsCodeDark,
        Self::VsCodeLight,
        Self::Obsidian,
        Self::ObsidianLight,
        Self::Dracula,
        Self::Nord,
        Self::AtomOneDark,
        Self::GitHubLight,
        Self::GitHubDark,
        Self::SolarizedLight,
        Self::ViraInspired,
    ];

    /// Stable slug used for `data-color-theme` and theme-picker values.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::VsCodeDark => "vscode-dark",
            Self::VsCodeLight => "vscode-light",
            Self::Obsidian => "obsidian",
            Self::ObsidianLight => "obsidian-light",
            Self::Dracula => "dracula",
            Self::Nord => "nord",
            Self::AtomOneDark => "atom-one-dark",
            Self::GitHubLight => "github-light",
            Self::GitHubDark => "github-dark",
            Self::SolarizedLight => "solarized-light",
            Self::ViraInspired => "vira-inspired",
        }
    }

    /// Human-readable name for theme pickers, such as `"VS Code Dark"`.
    pub fn label(self) -> &'static str {
        match self {
            Self::VsCodeDark => "VS Code Dark",
            Self::VsCodeLight => "VS Code Light",
            Self::Obsidian => "Obsidian Dark",
            Self::ObsidianLight => "Obsidian Light",
            Self::Dracula => "Dracula",
            Self::Nord => "Nord",
            Self::AtomOneDark => "Atom One Dark",
            Self::GitHubLight => "GitHub Light",
            Self::GitHubDark => "GitHub Dark",
            Self::SolarizedLight => "Solarized Light",
            Self::ViraInspired => "Vira-inspired (original)",
        }
    }

    /// Layout family emitted as `data-theme`.
    ///
    /// Themes in one family share typography and spacing, so switching between
    /// them never changes text metrics.
    pub fn css_family(self) -> &'static str {
        match self {
            Self::VsCodeDark | Self::VsCodeLight | Self::Obsidian | Self::ObsidianLight => {
                self.as_str()
            }
            Self::Dracula
            | Self::Nord
            | Self::AtomOneDark
            | Self::GitHubDark
            | Self::ViraInspired => "obsidian",
            Self::GitHubLight | Self::SolarizedLight => "vscode-light",
        }
    }
}

/// One color token of the editor's palette, for [`ThemeColors`]. Each maps to
/// a `--config-editor-*` CSS variable that the bundled themes set.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum ThemeColor {
    /// Editor background.
    Background,
    /// Toolbar, footer, and frame background.
    Panel,
    /// Borders and dividers.
    Border,
    /// Plain document text.
    Text,
    /// Secondary text.
    Muted,
    /// Accent (active file underline, focus).
    Accent,
    /// Error underlines.
    Error,
    /// Warning underlines.
    Warning,
    /// Info underlines.
    Info,
    /// Hint underlines.
    Hint,
    /// Toolbar text.
    ToolbarText,
    /// Line numbers.
    LineNumber,
    /// The caret line's number.
    LineNumberActive,
    /// Added-line gutter marker.
    Added,
    /// Modified-line gutter marker.
    Modified,
    /// Removed-lines gutter marker (by default the `Error` color).
    Removed,
    /// Caret line background.
    ActiveLine,
    /// Background of lines with errors.
    ErrorLine,
    /// Background of added lines.
    AddedLine,
    /// Background of modified lines.
    ModifiedLine,
    /// Selected text background.
    Selection,
    /// Caret.
    Caret,
    /// Comments.
    Comment,
    /// Keys.
    Key,
    /// Punctuation.
    Punctuation,
    /// Strings.
    String,
    /// Numbers, booleans, and null.
    Literal,
    /// Other values.
    Value,
    /// Anchors and aliases (`&base`, `*base`).
    Reference,
    /// Expressions in strings (`${var.name}`).
    Interpolation,
    /// Popup background (suggestions, hover, peek).
    Raised,
    /// Popup border.
    RaisedBorder,
    /// Popup text.
    RaisedText,
    /// Popup secondary text.
    RaisedMuted,
    /// Popup row hover.
    RaisedHover,
    /// Highlighted suggestion.
    CompletionActive,
    /// Suggestion label.
    CompletionKey,
    /// Suggestion type.
    CompletionType,
    /// Suggestion type on the highlighted suggestion.
    CompletionTypeActive,
    /// Suggestion description.
    CompletionDetail,
    /// Suggestion icon border.
    CompletionIconBorder,
    /// Suggestion icon.
    CompletionIconText,
    /// Matched letters on the highlighted suggestion.
    CompletionMatchActive,
    /// Problem tooltip text.
    TooltipText,
    /// Buttons and keys.
    ControlBackground,
    /// Button text.
    ControlText,
    /// Disabled button text.
    ControlDisabledText,
    /// Focus outline.
    FocusRing,
    /// File icon background.
    FileIconBackground,
    /// File icon letter.
    FileIconText,
    /// Footer error count.
    StatusError,
    /// Footer warning count.
    StatusWarning,
    /// Footer info count.
    StatusInfo,
    /// Footer hint count.
    StatusHint,
    /// Footer change count (by default the `Modified` color).
    StatusChanges,
    /// Focused diff marker outline.
    DiffFocusRing,
    /// Divider in the change preview.
    DiffPeekDivider,
    /// Removed lines in the change preview.
    DiffRemovedBackground,
    /// Removed text in the change preview.
    DiffRemovedText,
    /// Current lines in the change preview.
    DiffCurrentBackground,
    /// Current text in the change preview.
    DiffCurrentText,
    /// Scrollbar thumb.
    ScrollbarThumb,
    /// Suggestion popup shadow.
    PopupShadow,
    /// Change preview shadow.
    PeekShadow,
    /// Hover card shadow.
    HoverShadow,
    /// Find matches.
    SearchMatch,
    /// The current find match.
    SearchMatchCurrent,
    /// The current find match's outline.
    SearchMatchCurrentBorder,
    /// A jump's target (go to definition, a use, a problem, a line), shown
    /// for a second as it fades out.
    JumpFlash,
    /// Text fields (the find widget's).
    InputBackground,
    /// Text field borders.
    InputBorder,
    /// A pressed option button (match case, whole word, regex).
    ToggleActive,
    /// Read-only values and the gutter lock (by default the `Muted` color).
    Locked,
    /// Secret placeholder chips.
    SecretChip,
    /// Secret placeholder chip outlines and the gutter key.
    SecretChipBorder,
    /// Fold chevrons in the gutter.
    FoldToggle,
    /// The `…` chip after a folded line.
    FoldChip,
    /// The chip's text.
    FoldChipText,
    /// The documents view's tab strip.
    TabBar,
    /// An inactive tab.
    TabBackground,
    /// The active tab.
    TabActiveBackground,
    /// An inactive tab's name.
    TabText,
    /// The active tab's name.
    TabActiveText,
    /// Borders between tabs and under the strip.
    TabBorder,
    /// The line over the active tab.
    TabActiveBorder,
    /// A tab's unsaved-changes dot.
    TabDirty,
    /// The close button on a tab (by default none).
    TabCloseBackground,
    /// The close button under the pointer.
    TabCloseHoverBackground,
    /// The "+" after the tabs (by default the `TabBar` color).
    TabAddBackground,
    /// The "+" itself (by default the `TabText` color).
    TabAddText,
    /// The "+" under the pointer (by default the `TabBackground` color).
    TabAddHoverBackground,
    /// The "+" itself under the pointer (by default the `TabActiveText` color).
    TabAddHoverText,
    /// The toolbar (by default the `Panel` color).
    ToolbarBackground,
    /// The line under the toolbar (by default the `Border` color).
    ToolbarBorder,
    /// The footer (by default the `Panel` color).
    FooterBackground,
    /// The line over the footer (by default the `Border` color).
    FooterBorder,
    /// The footer's text (by default the `RaisedMuted` color).
    FooterText,
}

impl ThemeColor {
    /// Every token, in a stable order.
    pub const ALL: &'static [Self] = &[
        Self::Background,
        Self::Panel,
        Self::Border,
        Self::Text,
        Self::Muted,
        Self::Accent,
        Self::Error,
        Self::Warning,
        Self::Info,
        Self::Hint,
        Self::ToolbarText,
        Self::LineNumber,
        Self::LineNumberActive,
        Self::Added,
        Self::Modified,
        Self::Removed,
        Self::ActiveLine,
        Self::ErrorLine,
        Self::AddedLine,
        Self::ModifiedLine,
        Self::Selection,
        Self::Caret,
        Self::Comment,
        Self::Key,
        Self::Punctuation,
        Self::String,
        Self::Literal,
        Self::Value,
        Self::Reference,
        Self::Interpolation,
        Self::Raised,
        Self::RaisedBorder,
        Self::RaisedText,
        Self::RaisedMuted,
        Self::RaisedHover,
        Self::CompletionActive,
        Self::CompletionKey,
        Self::CompletionType,
        Self::CompletionTypeActive,
        Self::CompletionDetail,
        Self::CompletionIconBorder,
        Self::CompletionIconText,
        Self::CompletionMatchActive,
        Self::TooltipText,
        Self::ControlBackground,
        Self::ControlText,
        Self::ControlDisabledText,
        Self::FocusRing,
        Self::FileIconBackground,
        Self::FileIconText,
        Self::StatusError,
        Self::StatusWarning,
        Self::StatusInfo,
        Self::StatusHint,
        Self::StatusChanges,
        Self::DiffFocusRing,
        Self::DiffPeekDivider,
        Self::DiffRemovedBackground,
        Self::DiffRemovedText,
        Self::DiffCurrentBackground,
        Self::DiffCurrentText,
        Self::ScrollbarThumb,
        Self::PopupShadow,
        Self::PeekShadow,
        Self::HoverShadow,
        Self::SearchMatch,
        Self::SearchMatchCurrent,
        Self::SearchMatchCurrentBorder,
        Self::JumpFlash,
        Self::InputBackground,
        Self::InputBorder,
        Self::ToggleActive,
        Self::Locked,
        Self::SecretChip,
        Self::SecretChipBorder,
        Self::FoldToggle,
        Self::FoldChip,
        Self::FoldChipText,
        Self::TabBar,
        Self::TabBackground,
        Self::TabActiveBackground,
        Self::TabText,
        Self::TabActiveText,
        Self::TabBorder,
        Self::TabActiveBorder,
        Self::TabDirty,
        Self::TabCloseBackground,
        Self::TabCloseHoverBackground,
        Self::TabAddBackground,
        Self::TabAddText,
        Self::TabAddHoverBackground,
        Self::TabAddHoverText,
        Self::ToolbarBackground,
        Self::ToolbarBorder,
        Self::FooterBackground,
        Self::FooterBorder,
        Self::FooterText,
    ];

    /// The token's name: its CSS variable without `--config-editor-`, such
    /// as `"key"`. [`ThemeColors`] read from JSON are keyed by it.
    pub fn token(self) -> &'static str {
        Token::token(self)
    }

    /// The CSS variable this token sets, such as `"--config-editor-key"`.
    pub fn css_var(self) -> &'static str {
        match self {
            Self::Background => "--config-editor-bg",
            Self::Panel => "--config-editor-panel",
            Self::Border => "--config-editor-border",
            Self::Text => "--config-editor-text",
            Self::Muted => "--config-editor-muted",
            Self::Accent => "--config-editor-accent",
            Self::Error => "--config-editor-error",
            Self::Warning => "--config-editor-warning",
            Self::Info => "--config-editor-info",
            Self::Hint => "--config-editor-hint",
            Self::ToolbarText => "--config-editor-toolbar-text",
            Self::LineNumber => "--config-editor-line-number",
            Self::LineNumberActive => "--config-editor-line-active",
            Self::Added => "--config-editor-added",
            Self::Modified => "--config-editor-modified",
            Self::Removed => "--config-editor-removed",
            Self::ActiveLine => "--config-editor-active-line",
            Self::ErrorLine => "--config-editor-error-line",
            Self::AddedLine => "--config-editor-added-line",
            Self::ModifiedLine => "--config-editor-modified-line",
            Self::Selection => "--config-editor-selection",
            Self::Caret => "--config-editor-caret",
            Self::Comment => "--config-editor-comment",
            Self::Key => "--config-editor-key",
            Self::Punctuation => "--config-editor-punctuation",
            Self::String => "--config-editor-string",
            Self::Literal => "--config-editor-literal",
            Self::Value => "--config-editor-value",
            Self::Reference => "--config-editor-reference",
            Self::Interpolation => "--config-editor-interpolation",
            Self::Raised => "--config-editor-raised",
            Self::RaisedBorder => "--config-editor-raised-border",
            Self::RaisedText => "--config-editor-raised-text",
            Self::RaisedMuted => "--config-editor-raised-muted",
            Self::RaisedHover => "--config-editor-raised-hover",
            Self::CompletionActive => "--config-editor-completion-active",
            Self::CompletionKey => "--config-editor-completion-key",
            Self::CompletionType => "--config-editor-completion-type",
            Self::CompletionTypeActive => "--config-editor-completion-type-active",
            Self::CompletionDetail => "--config-editor-completion-detail",
            Self::CompletionIconBorder => "--config-editor-completion-icon-border",
            Self::CompletionIconText => "--config-editor-completion-icon-text",
            Self::CompletionMatchActive => "--config-editor-completion-match-active",
            Self::TooltipText => "--config-editor-tooltip-text",
            Self::ControlBackground => "--config-editor-control-bg",
            Self::ControlText => "--config-editor-control-text",
            Self::ControlDisabledText => "--config-editor-control-disabled-text",
            Self::FocusRing => "--config-editor-focus-ring",
            Self::FileIconBackground => "--config-editor-file-icon-bg",
            Self::FileIconText => "--config-editor-file-icon-text",
            Self::StatusError => "--config-editor-status-error",
            Self::StatusWarning => "--config-editor-status-warning",
            Self::StatusInfo => "--config-editor-status-info",
            Self::StatusHint => "--config-editor-status-hint",
            Self::StatusChanges => "--config-editor-status-changes",
            Self::DiffFocusRing => "--config-editor-diff-focus-ring",
            Self::DiffPeekDivider => "--config-editor-diff-peek-divider",
            Self::DiffRemovedBackground => "--config-editor-diff-removed-bg",
            Self::DiffRemovedText => "--config-editor-diff-removed-text",
            Self::DiffCurrentBackground => "--config-editor-diff-current-bg",
            Self::DiffCurrentText => "--config-editor-diff-current-text",
            Self::ScrollbarThumb => "--config-editor-scrollbar-thumb",
            Self::PopupShadow => "--config-editor-popup-shadow",
            Self::PeekShadow => "--config-editor-peek-shadow",
            Self::HoverShadow => "--config-editor-hover-shadow",
            Self::SearchMatch => "--config-editor-search-match",
            Self::SearchMatchCurrent => "--config-editor-search-match-current",
            Self::SearchMatchCurrentBorder => "--config-editor-search-match-current-border",
            Self::JumpFlash => "--config-editor-jump-flash",
            Self::InputBackground => "--config-editor-input-bg",
            Self::InputBorder => "--config-editor-input-border",
            Self::ToggleActive => "--config-editor-toggle-active",
            Self::Locked => "--config-editor-locked",
            Self::SecretChip => "--config-editor-secret-chip",
            Self::SecretChipBorder => "--config-editor-secret-chip-border",
            Self::FoldToggle => "--config-editor-fold-toggle",
            Self::FoldChip => "--config-editor-fold-chip",
            Self::FoldChipText => "--config-editor-fold-chip-text",
            Self::TabBar => "--config-editor-tab-bar",
            Self::TabBackground => "--config-editor-tab-bg",
            Self::TabActiveBackground => "--config-editor-tab-active-bg",
            Self::TabText => "--config-editor-tab-text",
            Self::TabActiveText => "--config-editor-tab-active-text",
            Self::TabBorder => "--config-editor-tab-border",
            Self::TabActiveBorder => "--config-editor-tab-active-border",
            Self::TabDirty => "--config-editor-tab-dirty",
            Self::TabCloseBackground => "--config-editor-tab-close-bg",
            Self::TabCloseHoverBackground => "--config-editor-tab-close-hover-bg",
            Self::TabAddBackground => "--config-editor-tab-add-bg",
            Self::TabAddText => "--config-editor-tab-add-text",
            Self::TabAddHoverBackground => "--config-editor-tab-add-hover-bg",
            Self::TabAddHoverText => "--config-editor-tab-add-hover-text",
            Self::ToolbarBackground => "--config-editor-toolbar-bg",
            Self::ToolbarBorder => "--config-editor-toolbar-border",
            Self::FooterBackground => "--config-editor-footer-bg",
            Self::FooterBorder => "--config-editor-footer-border",
            Self::FooterText => "--config-editor-footer-text",
        }
    }
}

impl Token for ThemeColor {
    const ALL: &'static [Self] = Self::ALL;
    const KIND: &'static str = "color";
    const VALUE: &'static str = "a CSS color string";
    const EXPECTING: &'static str =
        "an object of token: CSS color, such as { \"key\": \"#ffcc66\" }";
    const LISTED: &'static str = "ThemeColorToken (TypeScript) and ThemeColor::ALL (Rust)";

    fn css_var(self) -> &'static str {
        Self::css_var(self)
    }
}

/// Color overrides on top of a bundled [`EditorTheme`], for a custom palette.
///
/// ```
/// use leptos_config_editor::{ThemeColor, ThemeColors};
/// let brand = ThemeColors::new()
///     .set(ThemeColor::Background, "#101418")
///     .set(ThemeColor::Key, "#ffcc66");
/// assert_eq!(
///     brand.to_style(),
///     "--config-editor-bg: #101418; --config-editor-key: #ffcc66;"
/// );
/// ```
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct ThemeColors {
    colors: TokenValues<ThemeColor>,
}

impl ThemeColors {
    /// No overrides: the base theme as it is.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets `color` to a CSS color (`"#ffcc66"`, `"rgb(0 0 0 / 40%)"`),
    /// replacing an earlier value for it.
    #[must_use]
    pub fn set(mut self, color: ThemeColor, value: impl Into<String>) -> Self {
        self.colors.set(color, value.into());
        self
    }

    /// The overrides as inline CSS declarations. Values that could end the
    /// declaration (`;`, braces) are dropped.
    pub fn to_style(&self) -> String {
        self.colors.to_style()
    }

    /// The overrides as declarations set through the CSSOM (which a strict
    /// CSP allows), with the same values [`Self::to_style`] keeps.
    pub(crate) fn declarations(&self) -> crate::cssom::Declarations {
        self.colors.declare(crate::cssom::Declarations::new())
    }

    /// Whether no color is overridden.
    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }

    /// The overrides, in the order they were set.
    pub fn iter(&self) -> impl Iterator<Item = (ThemeColor, &str)> {
        self.colors.iter()
    }
}

/// A root's inline style: the `colors` and then the `chrome` overrides.
pub(crate) fn root_style(colors: &ThemeColors, chrome: &ThemeChrome) -> crate::cssom::Declarations {
    chrome.declare(colors.declarations())
}

/// A slug that is not a bundled [`EditorTheme`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownTheme(pub String);

impl std::fmt::Display for UnknownTheme {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "unknown editor theme \"{}\"", self.0)
    }
}

impl std::error::Error for UnknownTheme {}

impl FromStr for EditorTheme {
    type Err = UnknownTheme;

    /// Parses a slug from [`EditorTheme::as_str`], such as `"vscode-dark"`.
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|theme| theme.as_str() == name)
            .ok_or_else(|| UnknownTheme(name.to_string()))
    }
}

impl std::fmt::Display for EditorTheme {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

/// A theme reads from and writes to its slug (`"vscode-dark"`).
impl Serialize for EditorTheme {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for EditorTheme {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let slug = String::deserialize(deserializer)?;
        slug.parse().map_err(serde::de::Error::custom)
    }
}

/// Colors read from and write to an object of token: CSS color
/// (`{"key": "#ffcc66"}`), keyed by [`ThemeColor::token`].
impl Serialize for ThemeColors {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.colors.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ThemeColors {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        TokenValues::deserialize(deserializer).map(|colors| Self { colors })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_round_trips_through_its_slug() {
        for theme in EditorTheme::ALL {
            assert_eq!(theme.as_str().parse::<EditorTheme>(), Ok(*theme));
        }
        assert!("no-such-theme".parse::<EditorTheme>().is_err());
    }
}

#[cfg(test)]
mod color_tests {
    use super::ThemeColor;

    /// Every token is a variable the preset stylesheet defines.
    #[test]
    fn every_theme_color_is_a_preset_token() {
        let preset = include_str!("../css/base.css");
        for color in ThemeColor::ALL {
            assert!(
                preset.contains(&format!("{}:", color.css_var())),
                "{color:?}"
            );
        }
    }
}
