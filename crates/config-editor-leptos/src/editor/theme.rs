use std::str::FromStr;

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
    /// Suggestion description.
    CompletionDetail,
    /// Suggestion icon border.
    CompletionIconBorder,
    /// Suggestion icon.
    CompletionIconText,
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
    /// Text fields (the find widget's).
    InputBackground,
    /// Text field borders.
    InputBorder,
    /// A pressed option button (match case, whole word, regex).
    ToggleActive,
    /// Read-only values and the gutter lock (by default the `Muted` color).
    Locked,
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
        Self::Raised,
        Self::RaisedBorder,
        Self::RaisedText,
        Self::RaisedMuted,
        Self::RaisedHover,
        Self::CompletionActive,
        Self::CompletionKey,
        Self::CompletionType,
        Self::CompletionDetail,
        Self::CompletionIconBorder,
        Self::CompletionIconText,
        Self::TooltipText,
        Self::ControlBackground,
        Self::ControlText,
        Self::ControlDisabledText,
        Self::FocusRing,
        Self::FileIconBackground,
        Self::FileIconText,
        Self::StatusError,
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
        Self::InputBackground,
        Self::InputBorder,
        Self::ToggleActive,
        Self::Locked,
    ];

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
            Self::Raised => "--config-editor-raised",
            Self::RaisedBorder => "--config-editor-raised-border",
            Self::RaisedText => "--config-editor-raised-text",
            Self::RaisedMuted => "--config-editor-raised-muted",
            Self::RaisedHover => "--config-editor-raised-hover",
            Self::CompletionActive => "--config-editor-completion-active",
            Self::CompletionKey => "--config-editor-completion-key",
            Self::CompletionType => "--config-editor-completion-type",
            Self::CompletionDetail => "--config-editor-completion-detail",
            Self::CompletionIconBorder => "--config-editor-completion-icon-border",
            Self::CompletionIconText => "--config-editor-completion-icon-text",
            Self::TooltipText => "--config-editor-tooltip-text",
            Self::ControlBackground => "--config-editor-control-bg",
            Self::ControlText => "--config-editor-control-text",
            Self::ControlDisabledText => "--config-editor-control-disabled-text",
            Self::FocusRing => "--config-editor-focus-ring",
            Self::FileIconBackground => "--config-editor-file-icon-bg",
            Self::FileIconText => "--config-editor-file-icon-text",
            Self::StatusError => "--config-editor-status-error",
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
            Self::InputBackground => "--config-editor-input-bg",
            Self::InputBorder => "--config-editor-input-border",
            Self::ToggleActive => "--config-editor-toggle-active",
            Self::Locked => "--config-editor-locked",
        }
    }
}

/// Color overrides on top of a bundled [`EditorTheme`], for a custom palette.
///
/// ```
/// use config_editor_leptos::{ThemeColor, ThemeColors};
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
    colors: Vec<(ThemeColor, String)>,
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
        let value = value.into();
        match self
            .colors
            .iter_mut()
            .find(|(existing, _)| *existing == color)
        {
            Some((_, current)) => *current = value,
            None => self.colors.push((color, value)),
        }
        self
    }

    /// The overrides as inline CSS declarations. Values that could end the
    /// declaration (`;`, braces) are dropped.
    pub fn to_style(&self) -> String {
        self.colors
            .iter()
            .filter(|(_, value)| !value.contains([';', '{', '}']))
            .map(|(color, value)| format!("{}: {};", color.css_var(), value.trim()))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Whether no color is overridden.
    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }
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
        let preset = include_str!("../preset.css");
        for color in ThemeColor::ALL {
            assert!(
                preset.contains(&format!("{}:", color.css_var())),
                "{color:?}"
            );
        }
    }
}
