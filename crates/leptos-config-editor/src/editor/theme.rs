use std::str::FromStr;

/// Bundled editor color theme. `VsCodeDark` is the default and preserves the
/// original VS Code-like appearance; the other variants reuse the same layout
/// with different palettes.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum EditorTheme {
    #[default]
    VsCodeDark,
    VsCodeLight,
    Obsidian,
    ObsidianLight,
    Dracula,
    Nord,
    AtomOneDark,
    GitHubLight,
    GitHubDark,
    SolarizedLight,
    ViraInspired,
}

impl EditorTheme {
    /// All bundled themes, in the order the demo theme picker lists them.
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

impl FromStr for EditorTheme {
    type Err = ();

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|theme| theme.as_str() == name)
            .ok_or(())
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
