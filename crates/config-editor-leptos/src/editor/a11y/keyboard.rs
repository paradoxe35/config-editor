//! The editor's keys, one table: the crate README's Keyboard table is
//! written from it (`keyboard_tests`), and the textarea's
//! `aria-keyshortcuts` names its single chords that work in the text
//! (`shortcuts`), so what a screen reader is told and what the README says
//! are the same keys.

/// When a row's chords work in the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum When {
    /// Always.
    Always,
    /// When the text can be edited.
    Editable,
    /// Suggestions on (and the text editable).
    Suggestions,
    /// Find on (the `search` setting).
    Find,
    /// Find on and the text editable (replace).
    Replace,
    /// Go to line on.
    GoToLine,
    /// A save callback with the save key on.
    Save,
    /// The problems shown.
    Problems,
    /// The changes against the baseline shown.
    Changes,
    /// Folding on.
    Folding,
    /// While the baseline notice is open.
    Notice,
    /// In a `ConfigDocuments` view.
    Documents,
    /// Not in the text: in a list, a widget or a preview.
    Elsewhere,
}

/// A row of the table: its keys and action as the README writes them, and
/// when its single chords work in the text: one `When` for them all, or
/// one per chord in order.
pub(super) struct Row {
    pub(super) keys: &'static str,
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "the README's table is written from it by the tests"
        )
    )]
    pub(super) action: &'static str,
    pub(super) when: &'static [When],
}

const fn row(keys: &'static str, action: &'static str, when: &'static [When]) -> Row {
    Row { keys, action, when }
}

use When::*;

pub(super) const KEYS: &[Row] = &[
    row(
        "`Ctrl+Space`",
        "Open suggestions (with schema examples)",
        &[Suggestions],
    ),
    row(
        "`↑` `↓` `PageUp` `PageDown`, `Enter` / `Tab`",
        "Move through suggestions, accept",
        &[Elsewhere],
    ),
    row(
        "`Tab` after accepting a block",
        "Jump to the next empty value, or select the next value the block wrote",
        &[Editable],
    ),
    row(
        "`Tab` / `Shift+Tab`",
        "Indent / outdent the line or selected lines",
        &[Editable],
    ),
    row(
        "`Ctrl+M`",
        "Toggle Tab moves focus: `Tab` / `Shift+Tab` leave the editor instead of indenting",
        // Read-only, Tab always leaves: nothing to toggle.
        &[Editable],
    ),
    row(
        "`Escape`, then `Tab` / `Shift+Tab`",
        "Leave the editor once (when the `Escape` had nothing to close)",
        &[Always],
    ),
    row(
        "`Enter`",
        "New line with the right indentation; steps out of a just-typed string or element",
        &[Editable],
    ),
    row(
        "`Ctrl+Enter` / `Ctrl+Shift+Enter`",
        "Open a line below / above",
        &[Editable],
    ),
    row("`Ctrl+/`", "Toggle comments", &[Editable]),
    row(
        "`Alt+↑` / `Alt+↓`",
        "Move the line or selected lines up / down (a folded line with its hidden lines, staying folded; past a folded block, over all of it)",
        &[Editable],
    ),
    row(
        "`Shift+Alt+↑` / `Shift+Alt+↓`",
        "Copy the line or selected lines up / down",
        &[Editable],
    ),
    row(
        "`Ctrl+Shift+K`",
        "Delete the line or selected lines (the caret keeps its column)",
        &[Editable],
    ),
    row(
        "`Ctrl+]` / `Ctrl+[`",
        "Indent / outdent the line or selected lines",
        &[Editable],
    ),
    row(
        "`Ctrl+Z`, `Ctrl+Shift+Z` / `Ctrl+Y`",
        "Undo, redo: the last 500 steps, fewer in a large text (they keep up to 64 MB of earlier texts)",
        &[Editable],
    ),
    row("`F8` / `Shift+F8`", "Next / previous problem", &[Problems]),
    row(
        "`Alt+Enter` on a problem with a docs link",
        "Open its documentation in a new tab (else the hover card's Documentation link)",
        &[Always],
    ),
    row(
        "`Ctrl+K Ctrl+I`",
        "Show the schema docs of the key at the caret, read to screen readers (`Escape`); in `Ctrl+M` mode, `Tab` reaches its links",
        &[Always],
    ),
    row(
        "`Ctrl+K Ctrl+I` again",
        "Focus those docs (or the problems' tip): arrows, `PageUp` `PageDown`, `Home` `End` scroll it, `Tab` reaches its links, `Enter` or `Alt+Enter` opens its docs link, `Escape` returns to the text",
        &[Always],
    ),
    row(
        "`PageDown` / `PageUp` (`Shift` to select)",
        "Move the caret a page in the text; the host page never scrolls",
        &[Always],
    ),
    row(
        "`Alt+F5` / `Shift+Alt+F5`",
        "Next / previous change against the baseline (announced, as `F8` is)",
        &[Changes],
    ),
    row(
        "`Alt+F3` / `Shift+Alt+F3`",
        "Next / previous change, opening its preview with the focus on Revert",
        &[Changes],
    ),
    row(
        "`Enter` or `R` in a change preview",
        "Revert this change (`Escape` closes the preview and returns to the text)",
        &[Elsewhere],
    ),
    row(
        "`Ctrl+G`",
        "Go to line: type `line` or `line:column`, `Enter` to go, `Escape` to cancel",
        &[GoToLine],
    ),
    row(
        "`Ctrl+S`",
        "Save through `on_save` (left to the browser without it)",
        &[Save],
    ),
    row(
        "`F6` while the baseline notice is open",
        "Move the focus to its Keep mine button, and back to the text",
        &[Notice],
    ),
    row(
        "`Escape` in the baseline notice",
        "Keep mine: keep the text, diff it against the new baseline",
        &[Elsewhere],
    ),
    row(
        "`Ctrl+Shift+[` / `Ctrl+Shift+]`",
        "Fold the innermost block around the caret (again: the one around it) / unfold",
        &[Folding],
    ),
    row(
        "`Ctrl+K Ctrl+0` / `Ctrl+K Ctrl+J`",
        "Fold everything / unfold everything",
        &[Folding],
    ),
    row(
        "`Home`",
        "First non-blank character, then the line start",
        &[Always],
    ),
    row(
        "`Ctrl+C` / `Ctrl+X` with nothing selected",
        "Copy / cut the whole line (pasting it inserts above)",
        &[Always, Editable],
    ),
    row(
        "`Ctrl+F` / `Ctrl+H`",
        "Find / find and replace (the selection, on one line, becomes the query)",
        &[Find, Replace],
    ),
    row(
        "`Enter` / `Shift+Enter`, `F3` / `Shift+F3`",
        "Next / previous match",
        &[Find],
    ),
    row(
        "`Alt+C` / `Alt+W` / `Alt+R` in find",
        "Match case / whole word / regular expression",
        &[Elsewhere],
    ),
    row(
        "`Enter` / `Ctrl+Alt+Enter` in replace",
        "Replace the current match / all matches (one undo step)",
        &[Elsewhere],
    ),
    row(
        "`Escape`",
        "Close popups and the find widget, then collapse the selection",
        &[Always],
    ),
    row(
        "`Alt+PageDown` / `Alt+PageUp`",
        "Next / previous document in a `ConfigDocuments` view",
        &[Documents],
    ),
];

/// The `Ctrl` chords macOS's `Cmd` does not stand in for (it keeps
/// `Cmd+Space`; `Cmd+M` minimizes; `Cmd+Shift+Z` is the redo there).
pub(super) const CONTROL_ONLY: &[&str] = &["Ctrl+Space", "Ctrl+M", "Ctrl+Y"];

/// A key as `aria-keyshortcuts` spells it.
fn aria_key(key: &str) -> String {
    match key {
        "Ctrl" => "Control".to_string(),
        "Cmd" => "Meta".to_string(),
        "↑" => "ArrowUp".to_string(),
        "↓" => "ArrowDown".to_string(),
        other => other.to_string(),
    }
}

fn is_function_key(key: &str) -> bool {
    key.strip_prefix('F')
        .is_some_and(|number| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
}

/// The single chords a row's keys name, as the README writes them: the
/// keys between backticks with `Ctrl` or `Alt`, or a function key (plain
/// keys and `Shift` with a plain key are typing and moving, not
/// shortcuts; two-step chords cannot be spelled in `aria-keyshortcuts`).
pub(super) fn chords(keys: &str) -> Vec<&str> {
    keys.split('`')
        .skip(1)
        .step_by(2)
        .filter(|token| !token.contains(' ') && !token.is_empty())
        .filter(|token| {
            let parts: Vec<&str> = token.split('+').collect();
            let (key, modifiers) = parts.split_last().expect("a key");
            modifiers
                .iter()
                .any(|modifier| matches!(*modifier, "Ctrl" | "Alt"))
                || is_function_key(key)
        })
        .collect()
}

/// `chord` (`Ctrl+Shift+[`) as `aria-keyshortcuts` spells it, with its
/// `Cmd` twin where macOS takes one.
pub(super) fn aria_chords(chord: &str) -> Vec<String> {
    let spelled: Vec<String> = chord.split('+').map(aria_key).collect();
    let mut out = vec![spelled.join("+")];
    if chord.contains("Ctrl") && !CONTROL_ONLY.contains(&chord) {
        out.push(spelled.join("+").replace("Control", "Meta"));
    }
    out
}

/// Each single chord of the table with when it works in the text.
pub(super) fn gated_chords() -> Vec<(&'static str, When)> {
    KEYS.iter()
        .flat_map(|row| {
            let chords = chords(row.keys);
            assert!(
                row.when.len() == 1 || row.when.len() == chords.len(),
                "{}: one `When` or one per chord",
                row.keys
            );
            chords
                .into_iter()
                .enumerate()
                .map(|(index, chord)| (chord, row.when[index.min(row.when.len() - 1)]))
                .collect::<Vec<_>>()
        })
        .collect()
}
