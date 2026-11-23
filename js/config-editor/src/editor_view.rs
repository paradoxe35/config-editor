//! `DocumentEditorView`: one document's editor in a `ConfigDocuments`, from
//! `documents.editor(id)`, typed as the `EditorView` interface. It has the
//! read and act members of a `ConfigEditor` (the document, paths, ranges,
//! folds, focus, saving, the diff), through the same calls
//! (`handle_calls`). It looks the editor up on each call, so it follows a
//! document whose editor is mounted again (a new language or schema).

use leptos::prelude::*;
use leptos_config_editor::EditorHandle;
use wasm_bindgen::prelude::*;

use crate::documents_view::{State, Texts};
use crate::handle_calls;
use crate::settings::settings_result;
use crate::typings::{LineRanges, PathResult, RangeResult, SettingsResult};
use crate::values::context_arg;

/// The editor of one document of a `ConfigDocuments` (an `EditorView`).
///
/// ```js
/// const tls = documents.editor("tls");
/// tls.reveal("/cert");
/// tls.fold("/ciphers");
/// ```
#[wasm_bindgen(skip_typescript)]
pub struct DocumentEditorView {
    state: State,
    id: String,
    /// The listing of the document it was made for (see `Texts::listing`).
    listing: u64,
}

impl DocumentEditorView {
    /// The document's value with pending edits applied, while it is listed.
    fn current_value(&self) -> Option<String> {
        // An editor waiting to mount (listed with `editAsText`) holds the
        // document's text: mounted first, its edit converts with the rest.
        let _ = self.handle();
        leptos_config_editor::flush_pending_edits();
        self.texts().map(|texts| texts.value)
    }

    pub(crate) fn new(state: State, id: String, listing: u64) -> Self {
        Self { state, id, listing }
    }

    /// The document's text and baseline, while it is the one listed.
    fn texts(&self) -> Option<Texts> {
        self.state
            .texts_of(&self.id)
            .filter(|texts| texts.listing == self.listing)
    }

    /// The document's editor, once it has mounted (and while it is listed).
    fn handle(&self) -> Option<EditorHandle> {
        self.texts()?;
        untrack(|| self.state.handle.editor(&self.id))
    }
}

#[wasm_bindgen]
impl DocumentEditorView {
    /// The document, in its language: the host's `value`, then each edit;
    /// `null` once the document is no longer listed.
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> JsValue {
        self.current_value().map_or(JsValue::NULL, JsValue::from)
    }

    /// The settings of the document's editor: its own over the view's.
    #[wasm_bindgen(getter)]
    pub fn settings(&self) -> SettingsResult {
        let settings = self.texts().and_then(|_| self.state.settings_of(&self.id));
        settings_result(&settings.unwrap_or_default())
    }

    /// The unified diff (`diff -u`) of the document's baseline against it,
    /// with `context` unchanged lines around each change (default 3).
    #[wasm_bindgen(js_name = unifiedDiff)]
    pub fn unified_diff(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "number")] context: JsValue,
    ) -> Result<String, JsError> {
        let context = context_arg(&context)?;
        let current = self.current_value().unwrap_or_default();
        let baseline = self
            .texts()
            .map_or_else(|| current.clone(), |texts| texts.baseline);
        Ok(leptos_config_editor::unified_diff(
            &baseline, &current, context,
        ))
    }

    /// Where the field at `path` is written in the text on screen, or `null`.
    #[wasm_bindgen(js_name = rangeOf)]
    pub fn range_of(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FieldPath")] path: JsValue,
    ) -> Result<RangeResult, JsError> {
        handle_calls::range_of(self.handle(), &path)
    }

    /// The path of what the text on screen holds at string index `offset`.
    #[wasm_bindgen(js_name = pathAt)]
    pub fn path_at(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number")] offset: JsValue,
    ) -> Result<PathResult, JsError> {
        handle_calls::path_at(self.handle(), &offset)
    }

    /// The path of what the caret is on.
    #[wasm_bindgen(getter, js_name = currentPath)]
    pub fn current_path(&self) -> PathResult {
        handle_calls::current_path(self.handle())
    }

    /// The text on screen (in the `editAs` language while editing as
    /// another one); before the editor mounts, the document's `value`;
    /// `null` once the document is no longer listed.
    #[wasm_bindgen(getter)]
    pub fn text(&self) -> JsValue {
        handle_calls::text(self.handle())
            .or_else(|| self.texts().map(|texts| texts.value))
            .map_or(JsValue::NULL, JsValue::from)
    }

    /// The folded blocks, as one-based `{ startLine, endLine }`.
    #[wasm_bindgen(getter, js_name = foldedLines)]
    pub fn folded_lines(&self) -> LineRanges {
        handle_calls::folded_lines(self.handle())
    }

    /// Scrolls to the field at `path`, selects it and focuses the editor;
    /// `false` when there is no such field (or the editor has not mounted).
    /// Showing the document is `documents.select(id)`.
    pub fn reveal(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FieldPath")] path: JsValue,
    ) -> Result<bool, JsError> {
        handle_calls::reveal(self.handle(), &path)
    }

    /// Selects `range` (string indices `{ start, end }` into the text on
    /// screen, as `rangeOf` gives them: in the `editAs` language while
    /// editing as another one), scrolls it into view inside the editor and
    /// focuses the editor without moving the page, as `reveal` does for a
    /// field: any stretch, comments and blank lines included. A range past
    /// the end is clamped to it. `false` before the editor mounts and for an
    /// empty document.
    #[wasm_bindgen(js_name = revealRange)]
    pub fn reveal_range(
        &self,
        #[wasm_bindgen(unchecked_param_type = "{ start: number; end: number }")] range: JsValue,
    ) -> Result<bool, JsError> {
        handle_calls::reveal_range(self.handle(), &range)
    }

    /// Folds the block the field at `path` opens; `false` when there is none
    /// to fold.
    pub fn fold(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FieldPath")] path: JsValue,
    ) -> Result<bool, JsError> {
        handle_calls::fold(self.handle(), &path)
    }

    /// Opens the folds around the field at `path`; `false` when none was
    /// closed.
    pub fn unfold(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FieldPath")] path: JsValue,
    ) -> Result<bool, JsError> {
        handle_calls::unfold(self.handle(), &path)
    }

    /// Folds every block.
    #[wasm_bindgen(js_name = foldAll)]
    pub fn fold_all(&self) {
        handle_calls::fold_all(self.handle());
    }

    /// Opens every fold.
    #[wasm_bindgen(js_name = unfoldAll)]
    pub fn unfold_all(&self) {
        handle_calls::unfold_all(self.handle());
    }

    /// Focuses the editor, keeping its selection.
    pub fn focus(&self) {
        handle_calls::focus(self.handle());
    }

    /// Goes to the definition of the name at the caret, as `F12` does;
    /// `false` when the caret is on no known name.
    #[wasm_bindgen(js_name = goToDefinition)]
    pub fn go_to_definition(&self) -> bool {
        handle_calls::go_to_definition(self.handle())
    }

    /// Lists the uses of the name at the caret, as `Shift+F12` does;
    /// `false` when the caret is on no name.
    #[wasm_bindgen(js_name = findUses)]
    pub fn find_uses(&self) -> bool {
        handle_calls::find_uses(self.handle())
    }

    /// Makes the host's own change of the document, as
    /// `ConfigEditor.applyChanges` does (`options.revision` names the text
    /// the changes were made on), as `ConfigDocuments.applyChanges(id, …)`
    /// does: it throws while the document's editor is not mounted (it was
    /// not shown since it was listed); `null` when nothing was sent: no
    /// change, or the save is held (the host's change reaches the host with
    /// the next `onChange` once the save goes through).
    #[wasm_bindgen(js_name = applyChanges)]
    pub fn apply_changes(
        &self,
        #[wasm_bindgen(unchecked_param_type = "TextChange[]")] changes: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "ApplyChangesOptions")] options: JsValue,
    ) -> Result<Option<f64>, JsError> {
        let edits = crate::anchors::changes_from(&changes)?;
        let base = crate::anchors::change_base(&options)?;
        leptos_config_editor::js_support::flush_pending_input();
        let editor = crate::anchors::editor_to_change(self.handle(), &self.id)?;
        let revision = leptos_config_editor::js_support::apply_changes(
            &editor,
            &leptos_config_editor::js_support::HostEdits::Utf16(edits),
            base.as_ref(),
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
        self.state.took_host_change(&self.id, &editor);
        Ok(revision.map(|revision| revision as f64))
    }

    /// Saves the document as `Ctrl+S` in its editor does (calls the view's
    /// `onSave`); `false` when nothing was sent.
    pub fn save(&self) -> bool {
        leptos_config_editor::js_support::flush_pending_input();
        if self.state.saves.try_get_untracked() != Some(true) || self.texts().is_none() {
            return false;
        }
        untrack(|| self.state.handle.save(&self.id))
    }

    /// Whether the document differs from its baseline.
    #[wasm_bindgen(getter, js_name = isDirty)]
    pub fn is_dirty(&self) -> bool {
        leptos_config_editor::js_support::flush_pending_input();
        self.texts().is_some() && untrack(|| self.state.handle.is_dirty(&self.id))
    }
}
