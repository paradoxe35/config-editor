//! The notice while editing as another language: what of the host's file
//! the edited language does not show, and what typed there is not saved.
//!
//! What a save keeps, per the codec: the edited text is read into data,
//! which has no comments, and only the values that changed are written
//! into the host's text. So, for every pair of languages:
//!
//! - the host file's comments and formatting are not shown (the view is
//!   written fresh from the data) and are kept on save;
//! - comments typed in the edited language are never saved, whether the
//!   host's language has comments (YAML, TOML, HCL, XML) or not (JSON).

use super::super::language::ConfigLanguage;
use super::super::languages::Language;
use crate::EditorStrings;
use crate::strings::fill;

/// Why the notice shows; both may hold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::editor) struct NoticeReasons {
    /// The host's text has comments the edited language does not show.
    pub(in crate::editor) host_comments: bool,
    /// Among them, processing instructions (XML `<?…?>`): the notice names
    /// them too.
    pub(in crate::editor) host_instructions: bool,
    /// The edited text has comments, which a save drops.
    pub(in crate::editor) typed_comments: bool,
}

impl NoticeReasons {
    /// What the notice says for the host's text `host_text` edited as
    /// `edit`, the edited text now being `edit_text`. Nothing when `edit`
    /// is the host's language.
    pub(in crate::editor) fn of(
        host: Language,
        edit: Language,
        host_text: &str,
        edit_text: &str,
    ) -> Self {
        if host == edit {
            return Self::default();
        }
        Self {
            host_comments: host.behavior().has_comments(host_text),
            host_instructions: host.behavior().has_processing_instructions(host_text),
            typed_comments: edit.behavior().has_comments(edit_text),
        }
    }

    /// The reasons in `self` not in `dismissed`.
    pub(in crate::editor) fn without(self, dismissed: Self) -> Self {
        Self {
            host_comments: self.host_comments && !dismissed.host_comments,
            host_instructions: self.host_instructions && !dismissed.host_comments,
            typed_comments: self.typed_comments && !dismissed.typed_comments,
        }
    }

    pub(in crate::editor) fn any(self) -> bool {
        self.host_comments || self.typed_comments
    }

    /// The notice's sentences, worded by `strings`.
    pub(in crate::editor) fn messages(
        self,
        strings: &EditorStrings,
        host: Language,
        edit: Language,
    ) -> Vec<String> {
        let names: [(&str, &dyn std::fmt::Display); 2] =
            [("host", &host.label()), ("edit", &edit.label())];
        let mut messages = Vec::new();
        if self.host_comments {
            let template = if self.host_instructions {
                &strings.edit_as_host_instructions
            } else {
                &strings.edit_as_host_comments
            };
            messages.push(fill(template, &names));
        }
        if self.typed_comments {
            let template = if holds_comments(host.behavior()) {
                &strings.edit_as_typed_comments
            } else {
                &strings.edit_as_typed_comments_none
            };
            messages.push(fill(template, &names));
        }
        messages
    }
}

/// Whether a language has comments at all: `Ctrl+/` can comment a line.
pub(in crate::editor) fn holds_comments(behavior: &dyn ConfigLanguage) -> bool {
    behavior.toggle_comments("a", 0, 1).is_some()
}
