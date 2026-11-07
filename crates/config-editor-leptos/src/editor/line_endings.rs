//! A host file's line endings, kept: the editor works on LF text (the
//! textarea and all line math assume it), and what it hands back to the
//! host (`on_change`, `on_save`) is written with the CRLF line endings the
//! host's text had, in every language and through Edit-as.

use leptos::prelude::*;

use super::host_editing::SaveRequest;

/// Whether the host's text ends its lines with CRLF, as last seen.
#[derive(Clone, Copy)]
pub(super) struct LineEndings(StoredValue<bool>);

impl LineEndings {
    /// The line endings of `text` (LF for a text without line breaks).
    pub(super) fn of(text: &str) -> Self {
        Self(StoredValue::new(crlf(text).unwrap_or(false)))
    }

    /// A new text from the host: its line endings, when it has line breaks.
    pub(super) fn note(&self, text: &str) {
        if let Some(crlf) = crlf(text) {
            self.0.set_value(crlf);
        }
    }

    /// `text` (LF) written with the host's line endings.
    pub(super) fn restore(&self, text: String) -> String {
        if self.0.get_value() {
            text.replace("\r\n", "\n").replace('\n', "\r\n")
        } else {
            text
        }
    }

    /// The host's `on_change`, handed the text with its line endings.
    pub(super) fn on_change(self, callback: Callback<String>) -> Callback<String> {
        Callback::new(move |text: String| callback.run(self.restore(text)))
    }

    /// The host's `on_save`, handed the text with its line endings.
    pub(super) fn on_save(self, callback: Callback<SaveRequest>) -> Callback<SaveRequest> {
        Callback::new(move |request: SaveRequest| {
            callback.run(SaveRequest {
                text: self.restore(request.text),
                ..request
            });
        })
    }
}

/// Whether `text` ends most of its lines with CRLF; `None` without line
/// breaks.
fn crlf(text: &str) -> Option<bool> {
    let breaks = text.matches('\n').count();
    (breaks > 0).then(|| text.matches("\r\n").count() * 2 > breaks)
}

#[cfg(test)]
mod tests {
    use super::crlf;

    #[test]
    fn crlf_is_what_most_lines_end_with() {
        assert_eq!(crlf("a\r\nb\r\n"), Some(true));
        assert_eq!(crlf("a\nb\n"), Some(false));
        assert_eq!(crlf("a\r\nb\nc\r\n"), Some(true));
        assert_eq!(crlf("a\r\nb\nc\n"), Some(false));
        assert_eq!(crlf("a"), None);
    }
}

/// Edits in every language, in its own view and from every other, reach
/// `on_change` and `on_save` with the host's CRLF line endings (round 16
/// turned every line ending into LF).
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod editor_tests {
    use leptos::prelude::*;

    use super::super::edit::Edit;
    use super::super::host_editing::SaveRequest;
    use super::super::languages::Language;
    use super::super::sync::convert_now;
    use super::super::test_state::TestEditor;

    /// A commented document in `language`, CRLF, holding `a` = 1.
    fn sample(language: Language) -> &'static str {
        match language.label() {
            "YAML" => "# c\r\na: 1\r\nb: x\r\n",
            "JSON" => "{\r\n  \"a\": 1,\r\n  \"b\": \"x\"\r\n}\r\n",
            "TOML" => "# c\r\na = 1\r\nb = \"x\"\r\n",
            "HCL" => "# c\r\na = 1\r\nb = \"x\"\r\n",
            "XML" => "<!-- c -->\r\n<a>1</a>\r\n<b>x</b>\r\n",
            other => panic!("no sample for {other}"),
        }
    }

    #[test]
    fn edits_reach_the_host_with_its_crlf_line_endings() {
        let mut failures = Vec::new();
        for &host in Language::ALL {
            for &edit in Language::ALL {
                let text = sample(host);
                let changes = RwSignal::new(Vec::<String>::new());
                let saves = RwSignal::new(Vec::<String>::new());
                let mut editor = TestEditor::new(host, text);
                editor.edit_as = (edit != host).then_some(edit);
                editor.on_change = Some(Callback::new(move |text| {
                    changes.update(|all| all.push(text))
                }));
                editor.on_save = Some(Callback::new(move |request: SaveRequest| {
                    saves.update(|all| all.push(request.text));
                }));
                let (_owner, state) = editor.mount();
                let before = state.doc().text.get_untracked();
                let at = before.find('1').expect("the value");
                let after = format!("{}2{}", &before[..at], &before[at + 1..]);
                state.commit(Edit::new(before, (at, at + 1), after.clone(), at + 1));
                if edit != host {
                    convert_now(state, after, edit);
                }
                state.save();
                let expected = text.replacen('1', "2", 1);
                let case = format!("{host} host, edited as {edit}");
                if changes.get_untracked().last() != Some(&expected) {
                    failures.push(format!(
                        "{case}: on_change heard {:?}",
                        changes.get_untracked()
                    ));
                }
                if saves.get_untracked() != vec![expected.clone()] {
                    failures.push(format!("{case}: on_save got {:?}", saves.get_untracked()));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
