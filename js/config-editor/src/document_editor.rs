//! `DocumentEditor`: one document's editor in a `ConfigDocuments`, from
//! `documents.editor(id)`. It answers what a `ConfigEditor` answers about
//! its text (paths, ranges, folds, focus, saving), through the same calls
//! (`handle_calls`). It looks the editor up on each call, so it follows a
//! document whose editor is mounted again (a new language or schema).

use config_editor_leptos::EditorHandle;
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use crate::documents_view::State;
use crate::handle_calls;
use crate::typings::{LineRanges, PathResult, RangeResult};

/// The editor of one document of a `ConfigDocuments`.
///
/// ```js
/// const tls = documents.editor("tls");
/// tls.reveal("/cert");
/// tls.fold("/ciphers");
/// ```
#[wasm_bindgen]
pub struct DocumentEditor {
    state: State,
    id: String,
}

impl DocumentEditor {
    pub(crate) fn new(state: State, id: String) -> Self {
        Self { state, id }
    }

    /// The document's editor, once it has mounted.
    fn handle(&self) -> Option<EditorHandle> {
        untrack(|| self.state.handle.editor(&self.id))
    }
}

#[wasm_bindgen]
impl DocumentEditor {
    /// The document's id.
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> String {
        self.id.clone()
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
    /// another one); before the editor mounts, the document's `value`.
    #[wasm_bindgen(getter)]
    pub fn text(&self) -> String {
        handle_calls::text(self.handle())
            .or_else(|| self.state.value_of(&self.id))
            .unwrap_or_default()
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

    /// Saves the document as `Ctrl+S` in its editor does (calls the view's
    /// `onSave`); `false` when nothing was sent.
    pub fn save(&self) -> bool {
        config_editor_leptos::flush_pending_input();
        if self.state.saves.try_get_untracked() != Some(true) {
            return false;
        }
        untrack(|| self.state.handle.save(&self.id))
    }

    /// Whether the document differs from its baseline.
    #[wasm_bindgen(getter, js_name = isDirty)]
    pub fn is_dirty(&self) -> bool {
        config_editor_leptos::flush_pending_input();
        untrack(|| self.state.handle.is_dirty(&self.id))
    }
}
