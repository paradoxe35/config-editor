//! The open documents as the view holds them: one [`Entry`] per document of
//! the host's list, each with the live values its editor reads, and the
//! shared [`Documents`] context every part of the view acts through.

use leptos::html::Div;
use leptos::prelude::*;

use super::model::{CloseRequest, Document};
use super::plan::{Fate, SchemaCache, Setup, active_after, reconcile};
use crate::{EditorHandle, EditorTheme, Language, SaveRequest, ThemeColors};

/// One open document: what its tab and editor read. The editor mounts the
/// first time the document is shown (`visited`) and then stays mounted, so
/// switching back finds its caret, undo history, scroll and folds as they
/// were.
#[derive(Clone)]
pub(super) struct Entry {
    pub(super) id: String,
    /// Unique within the view, for DOM ids; a remount gets a new one.
    pub(super) serial: u64,
    pub(super) setup: Setup,
    pub(super) file_name: RwSignal<String>,
    pub(super) value: RwSignal<String>,
    pub(super) baseline: RwSignal<String>,
    pub(super) read_only: RwSignal<bool>,
    pub(super) edit_as: RwSignal<Option<Language>>,
    /// The text differs from the baseline: from the editor once mounted,
    /// from the host's text before.
    pub(super) dirty: RwSignal<bool>,
    /// Shown at least once: its editor is mounted.
    pub(super) visited: RwSignal<bool>,
    pub(super) handle: EditorHandle,
    /// Owns the signals above; disposed after the document closes.
    owner: Owner,
}

impl Entry {
    fn create(parent: &Owner, document: &Document, setup: Setup, serial: u64) -> Self {
        let owner = parent.child();
        let baseline = document
            .baseline
            .clone()
            .unwrap_or_else(|| document.value.clone());
        let dirty = differs(&document.value, &baseline);
        owner.with(|| Entry {
            id: document.id.clone(),
            serial,
            setup,
            file_name: RwSignal::new(document.file_name.clone()),
            value: RwSignal::new(document.value.clone()),
            baseline: RwSignal::new(baseline),
            read_only: RwSignal::new(document.read_only),
            edit_as: RwSignal::new(document.edit_as),
            dirty: RwSignal::new(dirty),
            visited: RwSignal::new(false),
            handle: EditorHandle::new(),
            owner: owner.clone(),
        })
    }

    /// Takes the host's new values for a document whose editor stays.
    fn update(&self, documents: Documents, document: &Document) {
        set_if_changed(self.file_name, &document.file_name);
        set_if_changed(self.value, &document.value);
        if let Some(baseline) = &document.baseline {
            set_if_changed(self.baseline, baseline);
        }
        if self.read_only.get_untracked() != document.read_only {
            self.read_only.set(document.read_only);
        }
        if self.edit_as.get_untracked() != document.edit_as {
            self.edit_as.set(document.edit_as);
        }
        if !untrack(|| self.handle.is_mounted()) {
            let dirty = self
                .value
                .with_untracked(|value| self.baseline.with_untracked(|base| differs(value, base)));
            documents.note_dirty(self, dirty);
        }
    }

    /// Frees the entry's signals once its tab and editor are gone.
    fn dispose_later(&self) {
        let owner = self.owner.clone();
        request_animation_frame(move || owner.cleanup());
    }

    /// The document's tab element id.
    pub(super) fn tab_id(&self, documents: Documents) -> String {
        format!("config-documents-{}-tab-{}", documents.prefix, self.serial)
    }

    /// The document's panel element id.
    pub(super) fn panel_id(&self, documents: Documents) -> String {
        format!(
            "config-documents-{}-panel-{}",
            documents.prefix, self.serial
        )
    }
}

/// Whether two texts differ, line endings aside.
fn differs(text: &str, baseline: &str) -> bool {
    text != baseline && text.replace("\r\n", "\n") != baseline.replace("\r\n", "\n")
}

fn set_if_changed(signal: RwSignal<String>, value: &str) {
    if signal.with_untracked(|current| current != value) {
        signal.set(value.to_string());
    }
}

/// The host's callbacks.
#[derive(Clone, Copy, Default)]
pub(super) struct Callbacks {
    pub(super) on_select: Option<Callback<String>>,
    pub(super) on_close: Option<Callback<CloseRequest>>,
    pub(super) on_change: Option<Callback<(String, String)>>,
    pub(super) on_save: Option<Callback<(String, SaveRequest)>>,
    pub(super) on_dirty_change: Option<Callback<(String, bool)>>,
}

/// What every editor of the view is built with.
#[derive(Clone, Copy)]
pub(super) struct EditorOptions {
    pub(super) theme: Signal<EditorTheme>,
    pub(super) colors: MaybeProp<ThemeColors>,
    pub(super) save_shortcut: Signal<bool>,
    pub(super) show_footer: bool,
    pub(super) show_diff: bool,
}

/// Where the focus goes when a document is selected.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Focus {
    /// Stays where it is.
    Keep,
    /// To the document's tab (keys in the tab list).
    Tab,
    /// Into the document's editor, its caret as it was (the switch keys
    /// from inside an editor, the handle).
    Editor,
}

/// The documents view's state, shared by its parts. `Copy`: a handful of
/// arena handles.
#[derive(Clone, Copy)]
pub(super) struct Documents {
    pub(super) entries: RwSignal<Vec<Entry>, LocalStorage>,
    pub(super) active: RwSignal<Option<String>>,
    /// A document whose editor takes the focus as soon as it is mounted.
    pub(super) focus_editor: RwSignal<Option<String>>,
    pub(super) callbacks: Callbacks,
    pub(super) options: EditorOptions,
    pub(super) root: NodeRef<Div>,
    pub(super) strip: NodeRef<Div>,
    /// Unique per view, for DOM ids.
    pub(super) prefix: u32,
    next_serial: StoredValue<u64>,
    schemas: StoredValue<SchemaCache, LocalStorage>,
    /// Owns the entries' signals.
    owner: StoredValue<Owner, LocalStorage>,
}

thread_local! {
    static NEXT_PREFIX: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

impl Documents {
    pub(super) fn new(callbacks: Callbacks, options: EditorOptions) -> Self {
        let prefix = NEXT_PREFIX.with(|next| {
            let prefix = next.get();
            next.set(prefix.wrapping_add(1));
            prefix
        });
        Self {
            entries: RwSignal::new_local(Vec::new()),
            active: RwSignal::new(None),
            focus_editor: RwSignal::new(None),
            callbacks,
            options,
            root: NodeRef::new(),
            strip: NodeRef::new(),
            prefix,
            next_serial: StoredValue::new(0),
            schemas: StoredValue::new_local(SchemaCache::default()),
            owner: StoredValue::new_local(Owner::current().unwrap_or_default()),
        }
    }

    /// The ids of the open documents, in order.
    pub(super) fn ids(&self) -> Vec<String> {
        self.entries
            .try_with_untracked(|entries| entries.iter().map(|entry| entry.id.clone()).collect())
            .unwrap_or_default()
    }

    /// The open document `id`.
    pub(super) fn entry(&self, id: &str) -> Option<Entry> {
        self.entries
            .try_with_untracked(|entries| entries.iter().find(|entry| entry.id == id).cloned())
            .flatten()
    }

    /// The active document's id.
    pub(super) fn active_id(&self) -> Option<String> {
        self.active.try_get_untracked().flatten()
    }

    /// Takes the host's list: keeps the editors of documents still listed
    /// (updating their text, baseline and read-only state), mounts a new one
    /// for a document whose language or schema changed, drops closed ones,
    /// and keeps a document active.
    pub(super) fn sync(&self, documents: &[Document]) {
        let previous_ids = self.ids();
        let schemas = documents
            .iter()
            .map(|document| document.schema.clone())
            .collect();
        let schemas = self
            .schemas
            .try_update_value(|cache| cache.resolve_all(schemas))
            .unwrap_or_default();
        let setups: Vec<Setup> = documents
            .iter()
            .zip(schemas)
            .map(|(document, schema)| Setup {
                language: document.resolved_language(),
                schema,
            })
            .collect();
        let existing = self.entries.get_untracked();
        let (fates, removed) = {
            let old: Vec<(&str, &Setup)> = existing
                .iter()
                .map(|entry| (entry.id.as_str(), &entry.setup))
                .collect();
            let new: Vec<(&str, &Setup)> = documents
                .iter()
                .zip(&setups)
                .map(|(document, setup)| (document.id.as_str(), setup))
                .collect();
            reconcile(&old, &new)
        };
        let owner = self.owner.get_value();
        let mut entries = Vec::with_capacity(documents.len());
        let mut added = Vec::new();
        for ((document, setup), fate) in documents.iter().zip(setups).zip(fates) {
            match fate {
                Fate::Keep(index) => {
                    let entry = existing[index].clone();
                    entry.update(*self, document);
                    entries.push(entry);
                }
                Fate::Remount(index) => {
                    let old = &existing[index];
                    old.dispose_later();
                    let mut document = document.clone();
                    // The new editor diffs against what the old one did.
                    if document.baseline.is_none() {
                        document.baseline = Some(old.baseline.get_untracked());
                    }
                    let entry = Entry::create(&owner, &document, setup, self.serial());
                    entry.dirty.set(old.dirty.get_untracked());
                    entries.push(entry);
                }
                Fate::Add => {
                    let entry = Entry::create(&owner, document, setup, self.serial());
                    added.push(entry.clone());
                    entries.push(entry);
                }
                Fate::Duplicate => web_sys::console::warn_1(
                    &format!(
                        "config-editor: document id \"{}\" is listed twice; the second is ignored",
                        document.id
                    )
                    .into(),
                ),
            }
        }
        for index in removed {
            existing[index].dispose_later();
        }
        let ids: Vec<String> = entries.iter().map(|entry| entry.id.clone()).collect();
        self.entries.set(entries);
        for entry in added {
            self.report_dirty(&entry.id, entry.dirty.get_untracked());
        }
        let active = self.active_id();
        let next = active_after(&previous_ids, &ids, active.as_deref());
        match next {
            Some(next) if active.as_deref() != Some(next.as_str()) => {
                let had_focus = self.has_focus();
                self.select(
                    &next,
                    if had_focus {
                        Focus::Editor
                    } else {
                        Focus::Keep
                    },
                );
            }
            Some(next) => self.show(&next),
            None => self.active.set(None),
        }
    }

    fn serial(&self) -> u64 {
        let serial = self.next_serial.get_value();
        self.next_serial.set_value(serial + 1);
        serial
    }

    /// Makes `id` the active document; tells the host when it was another
    /// one. Returns whether `id` is open.
    pub(super) fn select(&self, id: &str, focus: Focus) -> bool {
        if self.entry(id).is_none() {
            return false;
        }
        self.show(id);
        if self.active_id().as_deref() != Some(id) {
            self.active.set(Some(id.to_string()));
            if let Some(callback) = self.callbacks.on_select {
                callback.run(id.to_string());
            }
        }
        match focus {
            Focus::Keep => {}
            Focus::Tab => self.focus_tab(id),
            Focus::Editor => self.focus_editor.set(Some(id.to_string())),
        }
        true
    }

    /// Mounts the editor of `id` if it is not yet.
    fn show(&self, id: &str) {
        if let Some(entry) = self
            .entry(id)
            .filter(|entry| !entry.visited.get_untracked())
        {
            entry.visited.set(true);
        }
        if self.active_id().is_none() {
            self.active.set(Some(id.to_string()));
        }
    }

    /// Asks the host to close `id`; see [`CloseRequest`].
    pub(super) fn request_close(&self, id: &str) {
        let (Some(callback), Some(entry)) = (self.callbacks.on_close, self.entry(id)) else {
            return;
        };
        callback.run(CloseRequest {
            id: id.to_string(),
            dirty: entry.dirty.get_untracked(),
        });
    }

    /// Records whether `entry` has unsaved edits, telling the host when that
    /// changed.
    pub(super) fn note_dirty(&self, entry: &Entry, dirty: bool) {
        if entry.dirty.try_get_untracked() == Some(dirty) {
            return;
        }
        entry.dirty.set(dirty);
        self.report_dirty(&entry.id, dirty);
    }

    fn report_dirty(&self, id: &str, dirty: bool) {
        if let Some(callback) = self.callbacks.on_dirty_change {
            callback.run((id.to_string(), dirty));
        }
    }

    /// Saves `id` as `Ctrl+S` in its editor does; a document never shown
    /// sends the host's text as it is. Returns whether it was sent.
    pub(super) fn save(&self, id: &str) -> bool {
        let Some(entry) = self.entry(id) else {
            return false;
        };
        if untrack(|| entry.handle.is_mounted()) {
            return entry.handle.save();
        }
        let Some(callback) = self.callbacks.on_save else {
            return false;
        };
        callback.run((
            id.to_string(),
            SaveRequest {
                text: entry.value.get_untracked(),
                language: entry.setup.language,
            },
        ));
        true
    }

    /// Saves every document with unsaved edits; returns the ids sent.
    pub(super) fn save_all(&self) -> Vec<String> {
        self.ids()
            .into_iter()
            .filter(|id| {
                self.entry(id)
                    .is_some_and(|entry| entry.dirty.get_untracked())
            })
            .filter(|id| self.save(id))
            .collect()
    }

    /// Whether the focus is inside the view.
    fn has_focus(&self) -> bool {
        let Some(root) = self.root.get_untracked() else {
            return false;
        };
        let active = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.active_element());
        active.is_some_and(|active| root.contains(Some(&active)))
    }

    /// The tab element of `id`.
    pub(super) fn tab_element(&self, id: &str) -> Option<web_sys::HtmlElement> {
        use wasm_bindgen::JsCast;
        let entry = self.entry(id)?;
        let root = self.root.get_untracked()?;
        root.query_selector(&format!("#{}", entry.tab_id(*self)))
            .ok()
            .flatten()?
            .dyn_into()
            .ok()
    }

    /// Focuses the tab of `id`, once it is rendered.
    fn focus_tab(&self, id: &str) {
        let documents = *self;
        let id = id.to_string();
        request_animation_frame(move || {
            if let Some(tab) = documents.tab_element(&id) {
                let _ = tab.focus();
            }
        });
    }
}
