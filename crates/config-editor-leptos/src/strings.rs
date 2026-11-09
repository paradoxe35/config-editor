//! [`EditorStrings`]: every text the editor writes itself, so a host can
//! reword or translate it. It is the `strings` field of
//! [`EditorSettings`](crate::EditorSettings), defined here with the same
//! rules (doc comments, defaults, camelCase keys); the TypeScript type and
//! the strings tables of the READMEs are generated from this file
//! (`js/config-editor/tests/settings.rs`).
//!
//! Texts a host or a schema gives (a read-only reason, a secret hint, a
//! schema's descriptions) and what parsers and the schema validator report
//! are theirs, not the editor's, and are shown as they come.

#[cfg(test)]
mod scan_tests;
mod template;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod usage_tests;

pub use template::StringsProblem;
pub(crate) use template::{counted, counted_as, fill};

/// The English texts, made once: what the headless functions (such as
/// [`Schema::validate`](crate::Schema::validate)) word their messages with.
pub(crate) fn default_strings() -> &'static EditorStrings {
    static DEFAULT: std::sync::LazyLock<EditorStrings> =
        std::sync::LazyLock::new(EditorStrings::default);
    &DEFAULT
}

use serde::{Deserialize, Serialize};

/// Every built-in text of the editor and its documents' tabs, in English
/// by default; change any to reword or translate the editor.
///
/// Templates hold placeholders in braces, such as `{count}` or `{line}`;
/// each text accepts the placeholders of its default and no others
/// ([`EditorStrings::check`] lists the unknown ones; they are shown as
/// written). Counted texts come in pairs: `…_one` when the count is 1,
/// `…_other` otherwise.
///
/// ```
/// use config_editor_leptos::{EditorSettings, EditorStrings};
///
/// let settings = EditorSettings {
///     strings: EditorStrings {
///         no_problems: "Keine Fehler".into(),
///         errors_one: "{count} Fehler".into(),
///         errors_other: "{count} Fehler".into(),
///         ..EditorStrings::default()
///     },
///     ..EditorSettings::default()
/// };
/// assert!(settings.strings.check().is_empty());
/// ```
///
/// Every key exists in every build: the texts of a Cargo feature that is
/// off (`search`, `diff`, `folding`, `protect`) are simply never shown.
///
/// From JSON (and the npm package) any subset of keys may be given, the
/// others keeping their English defaults:
/// `{"strings": {"noProblems": "Keine Fehler"}}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EditorStrings {
    // The footer.
    /// The footer's suggestions hint; `{key}` is where the chord goes.
    pub suggestions_hint: String,
    /// The chord that opens suggestions, as the footer draws it: keys
    /// between `+`.
    pub suggestions_key: String,
    /// The footer's problem count when there are none.
    pub no_problems: String,
    /// One error, in the footer's problem count.
    pub errors_one: String,
    /// Several errors, in the footer's problem count.
    pub errors_other: String,
    /// One warning, in the footer's problem count.
    pub warnings_one: String,
    /// Several warnings, in the footer's problem count.
    pub warnings_other: String,
    /// One info, in the footer's problem count.
    pub infos_one: String,
    /// Several infos, in the footer's problem count.
    pub infos_other: String,
    /// One hint, in the footer's problem count.
    pub hints_one: String,
    /// Several hints, in the footer's problem count.
    pub hints_other: String,
    /// After the problem count when every problem has its place.
    pub problems_hint: String,
    /// `problems_hint` on a touch screen, which has no hover.
    pub problems_hint_touch: String,
    /// After the problem count when the editor's own problems were cut;
    /// `{count}` is how many of them are shown (the host's are all shown).
    pub problems_cut: String,
    /// The problem that says the list was cut, after `{count}` problems.
    pub problems_more: String,
    /// After the problem count when the problems have more underlines
    /// than are drawn; `{count}` is how many are (the most serious first).
    pub underlines_cut: String,
    /// The footer's change count against the baseline, for one change.
    pub changes_one: String,
    /// The footer's change count, for several changes.
    pub changes_other: String,
    /// The change count's tooltip when one line changed; `{lines}` lists
    /// the added, modified and removed counts.
    pub change_details_one: String,
    /// The change count's tooltip when several lines changed.
    pub change_details_other: String,
    /// Added lines, in the change count's tooltip.
    pub changed_added: String,
    /// Modified lines, in the change count's tooltip.
    pub changed_modified: String,
    /// Removed lines, in the change count's tooltip.
    pub changed_removed: String,
    /// The footer's line count, for one line.
    pub lines_one: String,
    /// The footer's line count, for several lines.
    pub lines_other: String,
    /// The footer's indicator while Tab moves the focus.
    pub tab_focus: String,
    /// Its tooltip; clicking it turns the mode off.
    pub tab_focus_title: String,
    /// Announced when `Ctrl+M` makes Tab move the focus.
    pub tab_focus_on: String,
    /// Announced when `Ctrl+M` makes Tab indent again.
    pub tab_focus_off: String,

    // The toolbar.
    /// The toolbar's schema state.
    pub schema_connected: String,

    // Edit-as.
    /// The Edit-as notice when the host's file has comments, which the
    /// edited language does not show but saving keeps.
    pub edit_as_host_comments: String,
    /// The same when the host's file has processing instructions too (XML
    /// `<?…?>`), which the edited language does not show either.
    pub edit_as_host_instructions: String,
    /// The Edit-as notice when comments are typed in the edited language,
    /// which saving drops, and the host's language has comments.
    pub edit_as_typed_comments: String,
    /// The same when the host's language has no comments (JSON).
    pub edit_as_typed_comments_none: String,
    /// The Edit-as notice's close button.
    pub edit_as_dismiss: String,

    // The baseline notice.
    /// The notice shown when the host's baseline changes under unsaved
    /// edits.
    pub baseline_changed: String,
    /// What the notice's choices do, after its message.
    pub baseline_changed_detail: String,
    /// The notice's button that takes the new baseline as the text.
    pub reload: String,
    /// Its tooltip.
    pub reload_title: String,
    /// The notice's button that keeps the text against the new baseline.
    pub keep_mine: String,
    /// Its tooltip.
    pub keep_mine_title: String,
    /// How to reach the notice's choices from the keyboard, shown on it
    /// (and announced with it).
    pub baseline_changed_keys: String,

    // Find and replace.
    /// The find widget's accessible name.
    pub find_widget: String,
    /// The button showing the replace row.
    pub toggle_replace: String,
    /// Its tooltip.
    pub toggle_replace_title: String,
    /// The query field's accessible name and placeholder.
    pub find: String,
    /// The match-case toggle.
    pub match_case: String,
    /// Its tooltip.
    pub match_case_title: String,
    /// The whole-word toggle.
    pub whole_word: String,
    /// Its tooltip.
    pub whole_word_title: String,
    /// The regular-expression toggle.
    pub use_regex: String,
    /// Its tooltip.
    pub use_regex_title: String,
    /// The previous-match button.
    pub previous_match: String,
    /// Its tooltip.
    pub previous_match_title: String,
    /// The next-match button.
    pub next_match: String,
    /// Its tooltip.
    pub next_match_title: String,
    /// The find widget's close button.
    pub close: String,
    /// Its tooltip.
    pub close_title: String,
    /// The replacement field's accessible name and placeholder, and the
    /// button replacing the current match.
    pub replace: String,
    /// The replace button's tooltip.
    pub replace_title: String,
    /// The replace-all button.
    pub replace_all: String,
    /// Its accessible name.
    pub replace_all_label: String,
    /// Its tooltip.
    pub replace_all_title: String,
    /// The match count: match `{current}` of `{total}`.
    pub match_position: String,
    /// The match count when nothing matches.
    pub no_results: String,
    /// The match count when the regular expression is invalid.
    pub invalid_pattern: String,
    /// After Replace All, for one match.
    pub replaced_one: String,
    /// After Replace All, for several.
    pub replaced_other: String,
    /// Added when Replace All left matches in protected text alone.
    pub replace_skipped: String,
    /// After Replace All when every match is in read-only text or a secret
    /// placeholder.
    pub nothing_replaced: String,

    // Go to line.
    /// The go-to-line field's accessible name.
    pub go_to_line: String,
    /// Its placeholder.
    pub go_to_line_placeholder: String,
    /// Its hint: the range of lines.
    pub go_to_line_hint: String,
    /// Its hint when the text is no position.
    pub go_to_line_invalid: String,

    // Changes against the baseline.
    /// The change preview's title for an added line.
    pub added_line: String,
    /// For a modified line.
    pub modified_line: String,
    /// For removed lines.
    pub removed_lines: String,
    /// For lines removed after the last one.
    pub removed_lines_at_end: String,
    /// The gutter marker of an added line.
    pub show_added: String,
    /// Its tooltip.
    pub show_added_title: String,
    /// The gutter marker of a modified line.
    pub show_modified: String,
    /// Its tooltip.
    pub show_modified_title: String,
    /// The gutter marker of removed lines.
    pub show_removed: String,
    /// Its tooltip, and that of lines removed at the end.
    pub show_removed_title: String,
    /// The gutter marker of lines removed after the last one.
    pub show_removed_at_end: String,
    /// The change preview's heading: its `{title}` and `{line}`.
    pub preview_heading: String,
    /// The change preview's accessible name.
    pub preview_label: String,
    /// The preview's revert button.
    pub revert: String,
    /// Its tooltip, for a one-line change.
    pub revert_title: String,
    /// Its tooltip, for a change over lines `{first}` to `{last}`.
    pub revert_lines_title: String,
    /// Its tooltip, for a changed line with removed lines.
    pub revert_line_and_removed_title: String,
    /// The preview's close button.
    pub close_preview: String,
    /// The preview's column of the baseline's lines.
    pub before: String,
    /// The preview's column of the current lines.
    pub current: String,

    // Folding.
    /// The chevron of an open range.
    pub fold: String,
    /// The chevron or chip of a closed fold.
    pub unfold: String,
    /// A closed fold's line number when it hides changes or problems.
    pub folded_changes: String,
    /// After a closed fold's chip label when it hides one problem;
    /// `{severity}` names it.
    pub folded_problem_one: String,
    /// After a closed fold's chip label when it hides `{count}` problems;
    /// `{severity}` names the most serious of them.
    pub folded_problem_other: String,

    // Hover and problems.
    /// The hover card's mark of a required key.
    pub required: String,
    /// The hover card's mark of a deprecated key.
    pub deprecated: String,
    /// The hover card's default row.
    pub default_value: String,
    /// The hover card's examples row.
    pub examples: String,
    /// The hover card's row listing the titles of the options (`oneOf`,
    /// `anyOf`) a field's object may take, while none matches it yet.
    pub one_of: String,
    /// The hover card's link to the schema's documentation.
    pub documentation: String,
    /// Announced when a second `Ctrl+K Ctrl+I` moves the focus into the
    /// hover card, to scroll it.
    pub hover_focused: String,
    /// The link under a host problem's message.
    pub open_docs: String,
    /// An error, as a screen reader hears it.
    pub error: String,
    /// A warning, as a screen reader hears it.
    pub warning: String,
    /// An info, as a screen reader hears it.
    pub info: String,
    /// A hint, as a screen reader hears it.
    pub hint: String,
    /// Who reported a host problem: the `{source}` of the problem texts
    /// below (empty for the editor's own problems).
    pub problem_source: String,
    /// A problem at the caret, for screen readers.
    pub problem: String,
    /// Announced on `F8`: problem `{position}` of `{total}`.
    pub problem_position: String,
    /// After a problem's text for screen readers: its `{code}` (from the
    /// host).
    pub problem_code: String,
    /// After a problem's text (or the hover card's docs) for screen
    /// readers: its documentation's `{url}`, and the key that opens it.
    pub problem_docs: String,
    /// An underlined problem's accessible name.
    pub problem_marker: String,
    /// What protected text at the caret is, for screen readers: its
    /// `{label}` and `{message}`.
    pub protected_note: String,

    // Accessible names.
    /// The text area.
    pub source_label: String,
    /// The suggestions list.
    pub suggestions_label: String,
    /// The layer of underlined problems.
    pub problems_label: String,
    /// The editor's frame.
    pub editor_label: String,

    // Read-only fields and secrets.
    /// Why a field is read-only when neither the schema nor the host says.
    pub read_only_field: String,
    /// Why text in a read-only range the host gave cannot change.
    pub read_only_text: String,
    /// Why renaming or moving the key above a read-only field is refused.
    pub holds_read_only: String,
    /// A save refused because it would change a read-only value.
    pub save_refused: String,
    /// A save waiting for the text to be valid: the language cannot say
    /// where the read-only field at `{pointer}` is written, so its value is
    /// checked only in a text that parses.
    pub save_waits: String,
    /// An edit refused because it makes a text that did not parse parse
    /// with the read-only field at `{pointer}` reading otherwise than in
    /// the host's text, and why the field is read-only (`{reason}`).
    pub read_only_changed: String,
    /// An edit refused because it makes a text that did not parse parse
    /// after more lines that may hold the read-only field at `{pointer}`
    /// were deleted from the host's text than can be checked through, and
    /// why the field is read-only (`{reason}`).
    pub read_only_unchecked: String,
    /// The gutter lock's label, before the reason.
    pub read_only_label: String,
    /// The gutter key's label, before the hint.
    pub secret_label: String,
    /// The hover text of a secret placeholder, unless the host gives one.
    pub secret_hint: String,
    /// What a schema problem says instead of a secret field's value.
    pub secret_value: String,
    /// A placeholder the host did not give, in a secret field.
    pub unknown_placeholder: String,
    /// One of the host's placeholders outside the field it stands in.
    pub moved_placeholder: String,
    /// The detail of the `${…}` reference suggestion.
    pub env_reference: String,
    /// The detail of the `file:` reference suggestion.
    pub file_reference: String,
    /// The detail of another reference suggestion.
    pub other_reference: String,
    /// The type column of a reference suggestion.
    pub reference_type: String,

    // Suggestions.
    /// The detail of the end tag XML suggests.
    pub closes_tag: String,
    /// Its type column.
    pub end_tag: String,

    // Problems the editor words.
    /// A key the schema does not allow (`{key}`); its object's pointer
    /// goes before it (`/web: `), as before every schema problem but at the
    /// root.
    pub key_not_allowed: String,
    /// Text in an element the schema wants elements in (XML); the element's
    /// pointer goes before it.
    pub text_not_allowed: String,
    /// A deprecated key.
    pub key_deprecated: String,
    /// A deprecated key whose schema says what to use instead.
    pub key_deprecated_instead: String,
    /// A value not among the allowed ones.
    pub not_one_of: String,
    /// A value of none of the accepted types.
    pub wrong_type: String,
    /// A schema that does not compile.
    pub invalid_schema: String,
    /// A secret placeholder pattern the editor cannot use (it does not
    /// compile, or matches empty text): no token is taken for a placeholder.
    pub invalid_placeholder: String,
    /// A host's problem with a field the editor cannot find in the text,
    /// listed with the field's path.
    pub problem_at_path: String,
    /// A value the host's language reads back differently (Edit-as).
    pub host_cannot_keep: String,
    /// A value the edited language cannot show exactly (Edit-as).
    pub edit_cannot_show: String,
    /// An edit the host's language cannot save (Edit-as).
    pub not_saved: String,
    /// A null the host's language cannot write (Edit-as).
    pub no_null: String,
    /// An infinity or NaN the host's language has no number for (Edit-as).
    pub no_non_finite: String,
    /// A host document that does not convert for editing.
    pub cannot_edit_as: String,
    /// An Edit-as switch waiting until the document converts.
    pub cannot_switch: String,
    /// An Edit-as switch refused: the host's language or the other one
    /// cannot write documents.
    pub switch_refused: String,
    /// XML text beside child elements, which no other language can show
    /// (Edit-as).
    pub mixed_content: String,

    // Documents.
    /// The tab list's accessible name, unless the host gives one.
    pub tab_list: String,
    /// Shown in place of an editor while no document is open.
    pub no_documents: String,
    /// A tab's close button.
    pub close_tab: String,
    /// Its tooltip.
    pub close_tab_title: String,
    /// The "+" button after the tabs (with `on_add`): its accessible name
    /// and tooltip.
    pub add_document: String,
    /// The dirty dot's tooltip, and what a tab's accessible name adds for
    /// it.
    pub unsaved: String,
    /// What a read-only document's tab adds to its accessible name.
    pub read_only_tab: String,
}

impl Default for EditorStrings {
    fn default() -> Self {
        let text = |text: &str| text.to_string();
        Self {
            suggestions_hint: text("{key} for suggestions"),
            suggestions_key: text("Ctrl+Space"),
            no_problems: text("No errors"),
            errors_one: text("{count} error"),
            errors_other: text("{count} errors"),
            warnings_one: text("{count} warning"),
            warnings_other: text("{count} warnings"),
            infos_one: text("{count} info"),
            infos_other: text("{count} info"),
            hints_one: text("{count} hint"),
            hints_other: text("{count} hints"),
            problems_hint: text("hover or focus underlined text"),
            problems_hint_touch: text("tap underlined text"),
            problems_cut: text("only the editor's first {count} are shown"),
            problems_more: text("Only the first {count} problems are shown; more follow."),
            underlines_cut: text("only {count} underlines are drawn, the most serious first"),
            changes_one: text("{count} change"),
            changes_other: text("{count} changes"),
            change_details_one: text("{lines} line · Alt+F5 for the next"),
            change_details_other: text("{lines} lines · Alt+F5 for the next"),
            changed_added: text("{count} added"),
            changed_modified: text("{count} modified"),
            changed_removed: text("{count} removed"),
            lines_one: text("{count} line"),
            lines_other: text("{count} lines"),
            tab_focus: text("Tab moves focus"),
            tab_focus_title: text("Tab moves focus (Ctrl+M to indent with Tab)"),
            tab_focus_on: text(
                "Tab moves focus: Tab and Shift+Tab now leave the editor. Ctrl+M to indent with Tab again.",
            ),
            tab_focus_off: text(
                "Tab indents: Tab and Shift+Tab indent again. Ctrl+M to move focus with Tab.",
            ),
            schema_connected: text("Schema connected"),
            edit_as_host_comments: text(
                "The {host} file's comments and formatting are not shown while editing as {edit}; saving keeps them.",
            ),
            edit_as_host_instructions: text(
                "The {host} file's comments, processing instructions and formatting are not shown while editing as {edit}; saving keeps them.",
            ),
            edit_as_typed_comments: text(
                "Comments typed in {edit} are not saved: the {host} file keeps only its own comments.",
            ),
            edit_as_typed_comments_none: text(
                "Comments typed in {edit} are not saved: {host} has no comments.",
            ),
            edit_as_dismiss: text("Dismiss"),
            baseline_changed: text("The file changed on the server."),
            baseline_changed_detail: text(
                "Reload it, losing your changes, or keep your text and compare it with the new version.",
            ),
            reload: text("Reload"),
            reload_title: text("Replace your text with the new version (Ctrl+Z brings yours back)"),
            keep_mine: text("Keep mine"),
            keep_mine_title: text(
                "Keep your text; the changes are now shown against the new version (Escape)",
            ),
            baseline_changed_keys: text("F6 moves to the choices and back."),
            find_widget: text("Find and replace"),
            toggle_replace: text("Toggle replace"),
            toggle_replace_title: text("Toggle replace (Ctrl+H)"),
            find: text("Find"),
            match_case: text("Match case"),
            match_case_title: text("Match case (Alt+C)"),
            whole_word: text("Match whole word"),
            whole_word_title: text("Match whole word (Alt+W)"),
            use_regex: text("Use regular expression"),
            use_regex_title: text("Use regular expression (Alt+R)"),
            previous_match: text("Previous match"),
            previous_match_title: text("Previous match (Shift+Enter)"),
            next_match: text("Next match"),
            next_match_title: text("Next match (Enter)"),
            close: text("Close"),
            close_title: text("Close (Escape)"),
            replace: text("Replace"),
            replace_title: text("Replace (Enter)"),
            replace_all: text("All"),
            replace_all_label: text("Replace all"),
            replace_all_title: text("Replace all (Ctrl+Alt+Enter)"),
            match_position: text("{current} of {total}"),
            no_results: text("No results"),
            invalid_pattern: text("Invalid pattern"),
            replaced_one: text("Replaced {count} occurrence"),
            replaced_other: text("Replaced {count} occurrences"),
            replace_skipped: text("; skipped {count} in read-only text or secret placeholders"),
            nothing_replaced: text(
                "Every match is in read-only text or secret placeholders; nothing was replaced",
            ),
            go_to_line: text("Go to line"),
            go_to_line_placeholder: text("line or line:column"),
            go_to_line_hint: text("Line 1–{lines}, or line:column"),
            go_to_line_invalid: text("Type a line number between 1 and {lines}"),
            added_line: text("Added line"),
            modified_line: text("Modified line"),
            removed_lines: text("Removed lines"),
            removed_lines_at_end: text("Removed lines at end of document"),
            show_added: text("Show added line {line}"),
            show_added_title: text("Show added line"),
            show_modified: text("Show before and after for modified line {line}"),
            show_modified_title: text("Show before and after"),
            show_removed: text("Show removed lines before line {line}"),
            show_removed_title: text("Show removed lines"),
            show_removed_at_end: text("Show removed lines at end of document"),
            preview_heading: text("{title} · line {line}"),
            preview_label: text("{title}, line {line}"),
            revert: text("Revert"),
            revert_title: text("Revert this change (R)"),
            revert_lines_title: text("Revert this change (lines {first}–{last}) (R)"),
            revert_line_and_removed_title: text(
                "Revert this change (line {line} and the removed lines) (R)",
            ),
            close_preview: text("Close change preview"),
            before: text("Before"),
            current: text("Current"),
            fold: text("Fold line {line}"),
            unfold: text("Unfold lines {first}–{last}"),
            folded_changes: text("Folded lines have changes"),
            folded_problem_one: text("hides a problem: {severity}"),
            folded_problem_other: text("hides {count} problems, the most serious: {severity}"),
            required: text("required"),
            deprecated: text("deprecated"),
            default_value: text("Default"),
            examples: text("Examples"),
            one_of: text("One of"),
            documentation: text("Documentation"),
            hover_focused: text(
                "In the docs: arrows, Page Up, Page Down, Home and End scroll them. Escape returns to the text.",
            ),
            open_docs: text("Open docs ↗ (Alt+Enter)"),
            error: text("Error"),
            warning: text("Warning"),
            info: text("Info"),
            hint: text("Hint"),
            problem_source: text(" from {source}"),
            problem: text("{severity}{source}: {message}"),
            problem_position: text("{severity} {position} of {total}{source}: {message}"),
            problem_code: text(" Code {code}."),
            problem_docs: text(" Documentation: {url} (Alt+Enter opens it)."),
            problem_marker: text("{language} {severity}{source} on line {line}: {message}"),
            protected_note: text("{label}: {message}"),
            source_label: text("{language} source"),
            suggestions_label: text("{language} suggestions"),
            problems_label: text("Inline {language} diagnostics"),
            editor_label: text("{language} editor"),
            read_only_field: text("This field is read-only"),
            read_only_text: text("This text is read-only"),
            holds_read_only: text("Holds a read-only field: {reason}"),
            save_refused: text("{pointer} is read-only and this change would alter it"),
            save_waits: text(
                "Not saved: {pointer} is read-only, and it can be checked only once the text is valid",
            ),
            read_only_changed: text(
                "This edit makes the text valid, but {pointer} would no longer read as the host gave it ({reason}): undo the change to it first",
            ),
            read_only_unchecked: text(
                "Too many lines were deleted while the text was invalid to check {pointer} ({reason}): undo deleting the lines that held it first",
            ),
            read_only_label: text("Read-only"),
            secret_label: text("Secret placeholder"),
            secret_hint: text("Keeps the current secret unless you replace it"),
            secret_value: text("the secret value"),
            unknown_placeholder: text(
                "{pointer}: unknown secret placeholder; it stands for no current secret, so write the new value instead",
            ),
            moved_placeholder: text(
                "{pointer}: this placeholder stands for the secret of {owner}, not this field's; here it is plain text",
            ),
            env_reference: text("Read the secret from an environment variable"),
            file_reference: text("Read the secret from a file"),
            other_reference: text("A reference starting with {prefix}"),
            reference_type: text("reference"),
            closes_tag: text("Closes <{name}>"),
            end_tag: text("end tag"),
            key_not_allowed: text("\"{key}\" is not allowed here"),
            text_not_allowed: text("text is not allowed here, only elements"),
            key_deprecated: text("{pointer}: \"{key}\" is deprecated"),
            key_deprecated_instead: text("{pointer}: \"{key}\" is deprecated. {message}"),
            not_one_of: text("{value} is not one of {allowed}"),
            wrong_type: text("{value} is not of type {types}"),
            invalid_schema: text("Invalid JSON Schema: {error}"),
            invalid_placeholder: text("secrets.placeholder: {error}"),
            problem_at_path: text("{path}: {message}"),
            host_cannot_keep: text(
                "{pointer}: {host} cannot keep this value exactly; it reads back differently",
            ),
            edit_cannot_show: text(
                "{pointer}: {edit} cannot show this value exactly; editing it here may change it",
            ),
            not_saved: text("Not saved as {host}: {error}"),
            no_null: text("{host} has no null value (at {pointer})"),
            no_non_finite: text("{host} has no infinity or NaN (at {pointer})"),
            cannot_edit_as: text("Cannot edit this {host} as {edit}: {error}"),
            cannot_switch: text("Cannot switch to {edit} yet: {error}"),
            switch_refused: text("Cannot edit as {edit}: {error}"),
            mixed_content: text(
                "{pointer}: text beside child elements cannot be shown in another language",
            ),
            tab_list: text("Open files"),
            no_documents: text("No open files"),
            close_tab: text("Close {name}"),
            close_tab_title: text("Close (Delete)"),
            add_document: text("New file"),
            unsaved: text("unsaved changes"),
            read_only_tab: text("read-only"),
        }
    }
}
