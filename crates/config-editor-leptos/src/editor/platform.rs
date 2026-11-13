//! The platform's name for the command key, so a shortcut hint names the key
//! the user has: `Cmd` on macOS and iOS, `Ctrl` elsewhere.
//!
//! A text writes `{cmd}` where the editor takes `Cmd` as well as `Ctrl` (see
//! the README's keys). A key it takes with `Ctrl` only (`Ctrl+Space`,
//! `Ctrl+M`) is written `Ctrl`, whatever the platform.

use std::sync::OnceLock;

/// `text` with `{cmd}` replaced by the command key's name here.
pub(super) fn name_command_key(text: &str) -> String {
    name_for(text, apple_platform())
}

fn name_for(text: &str, apple: bool) -> String {
    text.replace("{cmd}", if apple { "Cmd" } else { "Ctrl" })
}

/// Whether the browser runs on an Apple platform, read once.
fn apple_platform() -> bool {
    static APPLE: OnceLock<bool> = OnceLock::new();
    *APPLE.get_or_init(|| {
        web_sys::window().is_some_and(|window| {
            let navigator = window.navigator();
            [navigator.platform(), navigator.user_agent()]
                .into_iter()
                .flatten()
                .any(|name| is_apple(&name))
        })
    })
}

/// Whether a platform or user-agent string names macOS or iOS (an iPad
/// reports itself as a Mac).
fn is_apple(name: &str) -> bool {
    ["Mac", "iPhone", "iPad", "iPod"]
        .iter()
        .any(|word| name.contains(word))
}

#[cfg(test)]
mod tests {
    use super::{is_apple, name_for};

    #[test]
    fn the_command_key_is_named_after_the_platform() {
        let text = "Toggle replace ({cmd}+H)";
        assert_eq!(name_for(text, false), "Toggle replace (Ctrl+H)");
        assert_eq!(name_for(text, true), "Toggle replace (Cmd+H)");
        assert_eq!(name_for("Strg+Leertaste", true), "Strg+Leertaste");
        assert_eq!(name_for("{cmd}+Z, {cmd}+Y", true), "Cmd+Z, Cmd+Y");
    }

    #[test]
    fn apple_platforms_are_told_from_the_rest() {
        assert!(is_apple("MacIntel"));
        assert!(is_apple(
            "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)"
        ));
        assert!(!is_apple("Linux x86_64"));
        assert!(!is_apple("Win32"));
        assert!(!is_apple("Mozilla/5.0 (Linux; Android 14)"));
    }
}
