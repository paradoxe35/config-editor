use std::str::FromStr;

/// Bundled editor color theme. `VsCodeDark` is the default and preserves the
/// original VS Code-like appearance; the other variants reuse the same layout
/// with different palettes.
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

    /// Shared layout family (`data-theme`). Themes in the same family share
    /// typography and spacing so switching palettes never changes text metrics.
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
