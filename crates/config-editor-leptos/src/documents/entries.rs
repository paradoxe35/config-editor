//! The open documents as the view holds them: one [`Entry`] per document of
//! the host's list, each with the live values its editor reads, and the
//! shared [`Documents`] context every part of the view acts through.

use leptos::html::Div;
use leptos::prelude::*;

use super::adding::Adding;
use super::model::{CloseRequest, Document};
use super::plan::{Fate, SchemaCache, Setup, active_after, reconcile};
use crate::EditorStrings;
use crate::reactive::{Relay, detached_owner, later};
use crate::{
    Diagnostic, EditorHandle, EditorSettings, FieldPath, HostDiagnostic, Language, SaveRequest,
    SecretOptions, TextRange,
};
use std::sync::Arc;

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
    pub(super) read_only_reason: RwSignal<Option<String>>,
    /// The user may close it (with `on_close` set).
    pub(super) closable: RwSignal<bool>,
    pub(super) edit_as: RwSignal<Option<Language>>,
    /// The text its editor opens with (see `Document::edit_as_text`).
    pub(super) edit_as_text: StoredValue<Option<String>>,
    /// What the host was last told: the language on screen, and whether
    /// the text is written back to its value (see `Documents::note_language`, `note_sync`).
    pub(super) told: StoredValue<Told>,
    /// The document's own settings, else the view's.
    pub(super) settings: RwSignal<Option<EditorSettings>>,
    pub(super) host_diagnostics: RwSignal<Vec<HostDiagnostic>>,
    pub(super) read_only_paths: RwSignal<Vec<FieldPath>>,
    pub(super) read_only_ranges: RwSignal<Vec<TextRange>>,
    /// Taken when the editor mounts.
    pub(super) secrets: SecretOptions,
    /// The text differs from the baseline: from the editor once mounted,
    /// from the host's text before.
    pub(super) dirty: RwSignal<bool>,
    /// Shown at least once: its editor is mounted.
    pub(super) visited: RwSignal<bool>,
    /// Mounts its editor now, once its panel is on the page (set by the
    /// panel): a document shown answers its editor's calls at once.
    pub(super) mount_now: StoredValue<Option<Callback<()>>>,
    pub(super) handle: EditorHandle,
    /// Owns the signals above; disposed after the document closes, or with
    /// the view. No owner records it as a child: the view's owner lives on
    /// while documents open and close (see `reactive`).
    owner: Owner,
}

impl Entry {
    fn create(document: &Document, setup: Setup, serial: u64) -> Self {
        let owner = detached_owner();
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
            read_only_reason: RwSignal::new(document.read_only_reason.clone()),
            closable: RwSignal::new(document.closable),
            edit_as: RwSignal::new(document.edit_as),
            edit_as_text: StoredValue::new(document.edit_as_text.clone()),
            told: StoredValue::new(Told::default()),
            settings: RwSignal::new(document.settings.clone()),
            host_diagnostics: RwSignal::new(document.host_diagnostics.clone()),
            read_only_paths: RwSignal::new(document.read_only_paths.clone()),
            read_only_ranges: RwSignal::new(document.read_only_ranges.clone()),
            secrets: document.secrets.clone().unwrap_or_default(),
            dirty: RwSignal::new(dirty),
            visited: RwSignal::new(false),
            mount_now: StoredValue::new(None),
            handle: EditorHandle::new(),
            owner: owner.clone(),
        })
    }

    /// Its editor's `reveal`, `reveal_range` and `focus` select it first
    /// when it is not shown, as [`crate::DocumentsHandle::select`] does;
    /// selected but not yet on screen (its panel still inert, so the focus
    /// did not take), its editor is focused once it is.
    fn show_with(&self, documents: Documents) {
        let id = self.id.clone();
        let show = self.owner.with(|| {
            Callback::new(move |()| {
                if documents.active_id().as_deref() != Some(id.as_str()) {
                    documents.select(&id, Focus::Editor);
                } else if documents.panel_inert(&id) {
                    documents.focus_editor.set(Some(id.clone()));
                }
            })
        });
        self.handle.show_with(show);
    }

    /// Takes the host's new values for a document whose editor stays.
    fn update(&self, documents: Documents, document: &Document) {
        set_if_changed(self.file_name, &document.file_name);
        // A host lists every document again after each keystroke: only one
        // whose text or baseline changed is compared again for being dirty.
        let mut changed = set_if_changed(self.value, &document.value);
        if let Some(baseline) = &document.baseline {
            changed |= set_if_changed(self.baseline, baseline);
        }
        if self.read_only.get_untracked() != document.read_only {
            self.read_only.set(document.read_only);
        }
        set_if_changed(self.read_only_reason, &document.read_only_reason);
        if self.closable.get_untracked() != document.closable {
            self.closable.set(document.closable);
        }
        if self.edit_as.get_untracked() != document.edit_as {
            self.edit_as.set(document.edit_as);
        }
        if !untrack(|| self.handle.is_mounted()) {
            self.edit_as_text.set_value(document.edit_as_text.clone());
        }
        set_if_changed(self.settings, &document.settings);
        set_if_changed(self.host_diagnostics, &document.host_diagnostics);
        set_if_changed(self.read_only_paths, &document.read_only_paths);
        set_if_changed(self.read_only_ranges, &document.read_only_ranges);
        if changed && !untrack(|| self.handle.is_mounted()) {
            let dirty = self
                .value
                .with_untracked(|value| self.baseline.with_untracked(|base| differs(value, base)));
            documents.note_dirty(self, dirty);
        }
    }

    /// Frees the entry's signals once its tab and editor are gone.
    fn dispose_later(&self, documents: Documents) {
        let owner = self.owner.clone();
        // Gone with the view, the work drops the owner, which cleans it up
        // then.
        later::next_frame(documents, move |_| owner.cleanup());
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

/// Sets `signal` to `value` unless it holds it already; returns whether
/// it changed.
fn set_if_changed<T, V>(signal: RwSignal<T>, value: &V) -> bool
where
    T: std::borrow::Borrow<V> + Send + Sync + 'static,
    V: PartialEq + ToOwned<Owned = T> + ?Sized,
{
    let changed = signal.with_untracked(|current| current.borrow() != value);
    if changed {
        signal.set(value.to_owned());
    }
    changed
}

/// What the host was last told of a document's editor.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub(super) struct Told {
    pub(super) language: Option<Language>,
    pub(super) in_sync: Option<bool>,
}

/// The host's callbacks.
#[derive(Clone, Copy, Default)]
pub(super) struct Callbacks {
    pub(super) on_select: Option<Callback<String>>,
    pub(super) on_close: Option<Callback<CloseRequest>>,
    pub(super) on_add: Option<Callback<()>>,
    pub(super) on_change: Option<Callback<(String, String)>>,
    pub(super) on_save: Option<Callback<(String, SaveRequest)>>,
    pub(super) on_dirty_change: Option<Callback<(String, bool)>>,
    pub(super) on_problems: Option<Callback<(String, Vec<Diagnostic>)>>,
    pub(super) on_edit_as_change: Option<Callback<(String, Language)>>,
    pub(super) on_sync_change: Option<Callback<(String, bool)>>,
}

/// The view's state that each document's tab, panel and editor follow.
/// They come and go with their document, so they read it through relays
/// (see `reactive`).
#[derive(Clone, Copy)]
pub(super) struct Follows {
    pub(super) active: Relay<Option<String>>,
    pub(super) focus_editor: Relay<Option<String>>,
    /// The view's settings, for every document without its own.
    pub(super) settings: Relay<EditorSettings>,
    /// The view's built-in texts (the tabs').
    pub(super) strings: Relay<Arc<EditorStrings>>,
    /// Every tab's file name: what a tab shows of its own depends on the
    /// others' (see `tab_names`).
    pub(super) names: Relay<Vec<String>>,
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
    pub(super) follows: Follows,
    pub(super) root: NodeRef<Div>,
    pub(super) strip: NodeRef<Div>,
    /// Unique per view, for DOM ids.
    pub(super) prefix: u32,
    next_serial: StoredValue<u64>,
    schemas: StoredValue<SchemaCache, LocalStorage>,
    /// The ids listed twice the last time the list came: each is warned
    /// about once, until the list stops repeating it.
    repeated: StoredValue<Vec<String>>,
    /// The user asked for a new document (`on_add`) and the host has not
    /// added one yet: the next one added in answer is selected, its editor
    /// focused (see `adding`).
    pub(super) adding: Adding,
}

thread_local! {
    static NEXT_PREFIX: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Gone with the view's owner: deferred work skips it then (see
/// `reactive::later`).
impl IsDisposed for Documents {
    fn is_disposed(&self) -> bool {
        self.entries.is_disposed()
    }
}

impl Documents {
    pub(super) fn new(callbacks: Callbacks, settings: Signal<EditorSettings>) -> Self {
        let prefix = NEXT_PREFIX.with(|next| {
            let prefix = next.get();
            next.set(prefix.wrapping_add(1));
            prefix
        });
        let entries = RwSignal::new_local(Vec::<Entry>::new());
        let active = RwSignal::new(None);
        let focus_editor = RwSignal::new(None);
        // Entries are no owner's children: they go with the view here.
        on_cleanup(move || {
            if let Some(entries) = entries.try_get_untracked() {
                for entry in entries {
                    entry.owner.cleanup();
                }
            }
        });
        Self {
            entries,
            active,
            focus_editor,
            callbacks,
            follows: Follows {
                active: Relay::new(move || active.get()),
                focus_editor: Relay::new(move || focus_editor.get()),
                settings: Relay::new(move || settings.get()),
                strings: Relay::new(move || {
                    Arc::new(settings.with(|settings| settings.strings.clone()))
                }),
                names: Relay::new(move || {
                    entries
                        .with(|entries| entries.iter().map(|entry| entry.file_name.get()).collect())
                }),
            },
            root: NodeRef::new(),
            strip: NodeRef::new(),
            prefix,
            next_serial: StoredValue::new(0),
            schemas: StoredValue::new_local(SchemaCache::default()),
            repeated: StoredValue::new(Vec::new()),
            adding: Adding::new(),
        }
    }

    /// One of the view's built-in texts, as a view reads it: through a
    /// signal the calling view owns (see `reactive`).
    pub(super) fn text(
        self,
        pick: fn(&EditorStrings) -> &String,
    ) -> impl Fn() -> String + Copy + Send + Sync + 'static {
        let strings = self.follows.strings.read();
        move || strings.with(|strings| pick(strings).clone())
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
        let mut entries = Vec::with_capacity(documents.len());
        let mut added = Vec::new();
        let mut repeated = Vec::new();
        for ((document, setup), fate) in documents.iter().zip(setups).zip(fates) {
            match fate {
                Fate::Keep(index) => {
                    let entry = existing[index].clone();
                    entry.update(*self, document);
                    entries.push(entry);
                }
                Fate::Remount(index) => {
                    let old = &existing[index];
                    old.dispose_later(*self);
                    let mut document = document.clone();
                    // The new editor diffs against what the old one did.
                    if document.baseline.is_none() {
                        document.baseline = Some(old.baseline.get_untracked());
                    }
                    let entry = Entry::create(&document, setup, self.serial());
                    entry.show_with(*self);
                    entry.dirty.set(old.dirty.get_untracked());
                    entries.push(entry);
                }
                Fate::Add => {
                    let entry = Entry::create(document, setup, self.serial());
                    entry.show_with(*self);
                    added.push(entry.clone());
                    entries.push(entry);
                }
                Fate::Duplicate => repeated.push(document.id.clone()),
            }
        }
        let warned = self.repeated.get_value();
        for id in repeated.iter().filter(|id| !warned.contains(id)) {
            web_sys::console::warn_1(
                &format!(
                    "config-editor: document id \"{id}\" is listed twice; the second is ignored"
                )
                .into(),
            );
        }
        self.repeated.set_value(repeated);
        for index in removed {
            existing[index].dispose_later(*self);
        }
        let ids: Vec<String> = entries.iter().map(|entry| entry.id.clone()).collect();
        self.entries.set(entries);
        for entry in &added {
            self.report_dirty(&entry.id, entry.dirty.get_untracked());
        }
        // The document the host added for the user's "+": shown and edited.
        if let Some(entry) = added.first().filter(|_| self.adding.take()) {
            self.select(&entry.id, Focus::Editor);
            return;
        }
        let active = self.active_id();
        let next = active_after(&previous_ids, &ids, active.as_deref());
        match next {
            Some(next) if active.as_deref() != Some(next.as_str()) => {
                self.select(&next, self.focus_place_now());
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
        // The editor shown is hidden, inert, and would drop the focus to
        // the page: it goes to the editor shown instead.
        let focus = match focus {
            Focus::Keep
                if self.active_id().as_deref() != Some(id)
                    && self.focus_place_now() == Focus::Editor =>
            {
                Focus::Editor
            }
            focus => focus,
        };
        self.show(id);
        if self.active_id().as_deref() != Some(id) {
            self.active.set(Some(id.to_string()));
            if let Some(callback) = self.callbacks.on_select {
                callback.run(id.to_string());
            }
            // The host selected another document in answer: that one
            // stands, with its own focus.
            if self.active_id().as_deref() != Some(id) {
                return true;
            }
        }
        match focus {
            Focus::Keep => {}
            Focus::Tab => self.focus_tab(id),
            Focus::Editor => self.focus_editor.set(Some(id.to_string())),
        }
        true
    }

    /// Selects `id` for the user (a tab, a key): a "+" still waiting for
    /// its document no longer takes the focus when that comes.
    pub(super) fn pick(&self, id: &str, focus: Focus) -> bool {
        self.adding.stop();
        self.select(id, focus)
    }

    /// Mounts the editor of `id` if it is not yet.
    fn show(&self, id: &str) {
        if let Some(entry) = self
            .entry(id)
            .filter(|entry| !entry.visited.get_untracked())
        {
            entry.visited.set(true);
            if let Some(mount) = entry.mount_now.get_value() {
                untrack(|| mount.run(()));
            }
        }
        if self.active_id().is_none() {
            self.active.set(Some(id.to_string()));
        }
    }

    /// Whether the user may close `entry`: `on_close` is set and the
    /// document is closable. Reactive.
    pub(super) fn closes(&self, entry: &Entry) -> bool {
        self.callbacks.on_close.is_some() && entry.closable.get()
    }

    /// Asks the host to close `id`, when it may be closed; see
    /// [`CloseRequest`].
    pub(super) fn request_close(&self, id: &str) {
        let (Some(callback), Some(entry)) = (self.callbacks.on_close, self.entry(id)) else {
            return;
        };
        if !entry.closable.get_untracked() {
            return;
        }
        callback.run(CloseRequest {
            id: id.to_string(),
            dirty: entry.dirty.get_untracked(),
        });
    }

    /// Asks the host for a new document (the "+" button): the one it adds
    /// in answer is selected and its editor focused (see `adding`).
    pub(super) fn request_add(&self) {
        let Some(callback) = self.callbacks.on_add else {
            return;
        };
        self.adding.start(*self);
        callback.run(());
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

    /// Tells the host the language on screen in `entry`'s editor, when it
    /// is not what it was told last.
    pub(super) fn note_language(&self, entry: &Entry, language: Language) {
        let told = Some(language);
        if entry.told.try_with_value(|known| known.language) == Some(told) {
            return;
        }
        entry.told.update_value(|known| known.language = told);
        if let Some(callback) = self.callbacks.on_edit_as_change {
            callback.run((entry.id.clone(), language));
        }
    }

    /// Tells the host whether `entry`'s text on screen is written back to
    /// its value, when that is not what it was told last.
    pub(super) fn note_sync(&self, entry: &Entry, in_sync: bool) {
        // The language on screen is told first, when the editor's own
        // report of its sync state comes before the view's of its mount.
        if entry.told.try_with_value(|known| known.language.is_none()) == Some(true)
            && let Some(language) = untrack(|| entry.handle.language_on_screen())
        {
            self.note_language(entry, language);
        }
        let told = Some(in_sync);
        if entry.told.try_with_value(|known| known.in_sync) == Some(told) {
            return;
        }
        entry.told.update_value(|known| known.in_sync = told);
        if let Some(callback) = self.callbacks.on_sync_change {
            callback.run((entry.id.clone(), in_sync));
        }
    }

    /// Asks for `language` on screen in the document `id`, as a new
    /// `edit_as` does, and again when it is the one asked for last (see
    /// [`crate::DocumentsHandle::edit_as`]). Returns whether `id` is open.
    pub(super) fn edit_as(&self, id: &str, language: Language) -> bool {
        let Some(entry) = self.entry(id) else {
            return false;
        };
        let wanted = (language != entry.setup.language).then_some(language);
        if entry.edit_as.get_untracked() != wanted {
            entry.edit_as.set(wanted);
        }
        entry.handle.ask_edit_as(language);
        true
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

    /// Where the focus goes when the active document closes or the host
    /// shows another: to the tab taking its place when it was in the tab
    /// list (the WAI-ARIA tabs pattern: a second `Delete` closes that tab,
    /// it never edits its text), into the editor shown when it was
    /// elsewhere in the view (the editor going), and nowhere when it was
    /// outside (the host's page).
    fn focus_place_now(&self) -> Focus {
        // Not mounted. Mounted, the closed tab still holds the focus here:
        // the tab list renders the new list later.
        let Some(root) = self.root.get_untracked() else {
            return Focus::Keep;
        };
        let active = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.active_element());
        let inside = |node: Option<web_sys::HtmlDivElement>| {
            node.zip(active.as_ref())
                .is_some_and(|(node, active)| node.contains(Some(active)))
        };
        focus_place(inside(self.strip.get_untracked()), inside(Some(root)))
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

    /// Whether the panel of `id` is still inert (it is shown on the next
    /// render).
    fn panel_inert(&self, id: &str) -> bool {
        let (Some(entry), Some(root)) = (self.entry(id), self.root.get_untracked()) else {
            return false;
        };
        root.query_selector(&format!("#{}", entry.panel_id(*self)))
            .ok()
            .flatten()
            .is_some_and(|panel| panel.has_attribute("inert"))
    }

    /// Focuses the tab of `id`, once it is rendered.
    fn focus_tab(&self, id: &str) {
        let documents = *self;
        let id = id.to_string();
        later::next_frame(documents, move |documents| {
            if let Some(tab) = documents.tab_element(&id) {
                let _ = crate::focus::in_place(&tab);
            }
        });
    }
}

/// [`Documents::focus_place_now`] from where the focus is: in the tab
/// list, elsewhere in the view.
pub(super) fn focus_place(in_tab_list: bool, in_view: bool) -> Focus {
    if in_tab_list {
        Focus::Tab
    } else if in_view {
        Focus::Editor
    } else {
        Focus::Keep
    }
}

// The tests need a built-in language.
#[cfg(all(
    test,
    any(
        feature = "yaml",
        feature = "json",
        feature = "toml",
        feature = "hcl",
        feature = "xml"
    )
))]
#[path = "entries_tests.rs"]
mod tests;
