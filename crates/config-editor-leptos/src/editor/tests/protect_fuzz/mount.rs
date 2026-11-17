//! A live test editor on a case, its field protected one way, shown in
//! its own language or edited as another, with what the host heard.

use leptos::prelude::*;

use super::cases::{CASES, Case, Lock, schema, value};
use crate::editor::Language;
use crate::editor::host_editing::SaveRequest;
use crate::editor::paths::{FieldPath, TextPosition, TextRange};
use crate::editor::schema::build_schema;
use crate::editor::secret_options::SecretOptions;
use crate::editor::state::EditorState;
use crate::editor::test_state::TestEditor;

/// What a run starts from.
#[derive(Clone, Copy, Debug)]
pub(in crate::editor) struct Setup {
    /// The case (`CASES`), in the language on screen.
    pub(in crate::editor) case: usize,
    pub(in crate::editor) lock: Lock,
    /// The host's language when it is another (Edit-as).
    pub(in crate::editor) host: Option<Language>,
    /// Auto-closing brackets and quotes.
    pub(in crate::editor) pairs: bool,
}

impl Setup {
    pub(super) fn case(&self) -> &'static Case {
        &CASES[self.case]
    }

    pub(super) fn host_language(&self) -> Language {
        self.host.unwrap_or(self.case().language)
    }

    pub(super) fn view_language(&self) -> Language {
        self.case().language
    }
}

/// A mounted editor and what its host heard.
pub(super) struct Mounted {
    pub(super) _owner: Owner,
    pub(super) state: EditorState,
    pub(super) changes: RwSignal<Vec<String>>,
    pub(super) saves: RwSignal<Vec<String>>,
    /// The host's text as mounted.
    pub(super) host_text: String,
}

/// The case's text in its own language.
pub(super) fn case_text(setup: &Setup) -> String {
    setup.case().text.replace("{v}", value(setup.lock))
}

/// Mounts `setup`; `None` when the case cannot be written in the host's
/// language.
pub(super) fn mount(setup: &Setup) -> Option<Mounted> {
    let case = setup.case();
    let own = case_text(setup);
    let host_text = match setup.host {
        Some(host) => case.language.convert(&own, host).ok()?.text,
        None => own,
    };
    if host_text.matches(value(setup.lock)).count() != 1 {
        return None;
    }
    let changes = RwSignal::new(Vec::<String>::new());
    let saves = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(setup.host_language(), &host_text);
    if setup.host.is_some() {
        editor.edit_as = Some(case.language);
    }
    editor.on_change = Some(Callback::new(move |text| {
        changes.update(|all| all.push(text));
    }));
    editor.on_save = Some(Callback::new(move |request: SaveRequest| {
        saves.update(|all| all.push(request.text));
    }));
    match setup.lock {
        Lock::Secret => {
            editor.secrets = Some(SecretOptions::default());
            editor.schema = Some(std::sync::Arc::new(
                build_schema(&schema(case.pointer, setup.lock)).ok()?,
            ));
        }
        Lock::Path => editor.read_only_paths = vec![FieldPath::from(case.pointer)],
        Lock::Schema => {
            editor.schema = Some(std::sync::Arc::new(
                build_schema(&schema(case.pointer, setup.lock)).ok()?,
            ));
        }
        Lock::Range => editor.read_only_ranges = vec![line_range(&host_text, value(setup.lock))],
    }
    let (owner, state) = editor.mount();
    Some(Mounted {
        _owner: owner,
        state,
        changes,
        saves,
        host_text,
    })
}

/// The whole line of `text` holding `needle`, as a range.
pub(super) fn line_range(text: &str, needle: &str) -> TextRange {
    let at = text.find(needle).unwrap_or_default();
    let line = text[..at].matches('\n').count() + 1;
    let content = text.split('\n').nth(line - 1).unwrap_or_default();
    TextRange::new(
        TextPosition::new(line, 1),
        TextPosition::new(line, content.chars().count() + 1),
    )
}
