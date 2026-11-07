// What a framework component does with an editor, written once: the React,
// Vue and Svelte packages are thin wrappers around these bindings.
//
// A binding is fed the component's props (`attach` once its element exists,
// `update` after every change, `detach` on unmount) and decides what each
// change means: settings and most props apply live (the caret, undo history
// and folds stay), the few the editor is built from re-mount it (the
// document, its baseline and the caret carried over), the bound value goes
// through `ValueSync` (a late echo never rolls the document back), a mount
// that failed on a bad prop is retried when a prop changes, and the
// WebAssembly loads once for every editor on the page.

import init, { ConfigDocuments, ConfigEditor, unifiedDiff } from "./pkg/config_editor.js";
import { SETTING_TYPES } from "./setting-types.js";
export { SETTING_TYPES };
import { ValueSync } from "./sync.js";

let loading;
// Bindings whose own load failed, retried when a later load succeeds.
const onLoaded = new Set();

function loaded(value) {
  for (const retry of [...onLoaded]) retry();
  return value;
}

function forget(error) {
  loading = undefined;
  throw error;
}

/**
 * Loads the WebAssembly module once (`init`, shared by every binding).
 * @param {Parameters<typeof init>[0]} [input]
 */
export function loadConfigEditor(input) {
  // A URL given while a load runs: tried when that one fails (a wrong URL,
  // or the default one, followed by the right one). A failed load is not
  // kept: calling again (with the right URL) retries.
  if (loading && input !== undefined) {
    const previous = loading;
    loading = previous.catch(() => init(input)).then(loaded, forget);
    return loading;
  }
  loading ??= init(input).then(loaded, forget);
  return loading;
}

/** The settings among `props`: the keys of `EditorSettings` it sets, as plain data. */
function settingsOf(props) {
  const settings = {};
  for (const key of Object.keys(SETTING_TYPES)) {
    if (props[key] !== undefined) settings[key] = props[key];
  }
  return JSON.stringify(settings);
}

/** What the editor and documents bindings share: mounting, retries, errors. */
class Binding {
  #name;
  #generation = 0;
  #mountKey = null;
  #retryKey = null;
  #settings = null;
  #onLoad = null;

  constructor(name, props) {
    this.#name = name;
    /** The props as last given. */
    this.props = props ?? {};
    /** The element the component renders, while attached. */
    this.element = null;
    /** The `@pngwasi/config-editor` instance, while mounted. */
    this.instance = null;
    /** Whether the last mount threw (a bad prop): a changed prop retries it. */
    this.failed = false;
  }

  /** The component's element exists: load the module and mount into it. */
  attach(element, props = this.props) {
    this.element = element;
    this.props = props;
    this.#mount();
  }

  /** The props changed (call after every change; unchanged ones cost nothing). */
  update(props) {
    this.props = props;
    if (!this.element) return;
    if (this.mountKey() !== this.#mountKey) return this.#mount();
    const settings = settingsOf(props);
    if (this.#settings !== null && settings !== this.#settings) {
      this.#settings = settings;
      if (this.failed) return this.#mount();
      this.guard((instance) => instance.setSettings(JSON.parse(settings)));
    }
    const retryKey = this.retryKey();
    if (retryKey !== this.#retryKey) {
      this.#retryKey = retryKey;
      if (this.failed) return this.#mount();
    }
    this.sync(false);
  }

  /** The component unmounts: the instance is destroyed and freed. */
  detach() {
    this.#generation += 1;
    this.#cancelRetry();
    this.unmount();
    this.element = null;
  }

  /** Reports an error to `onError`, else to the console. */
  report(error) {
    if (this.props.onError) this.props.onError(error);
    else console.error(`${this.#name}:`, error);
  }

  /** Runs `apply` on the instance, if mounted, reporting what it throws. */
  guard(apply) {
    const instance = this.instance;
    if (!instance) return;
    try {
      apply(instance);
    } catch (error) {
      this.report(error);
    }
  }

  /** Runs `query` on the instance; `fallback` before it mounts or when it throws. */
  ask(query, fallback) {
    const instance = this.instance;
    if (!instance) return fallback;
    try {
      return query(instance);
    } catch (error) {
      this.report(error);
      return fallback;
    }
  }

  #cancelRetry() {
    if (this.#onLoad) onLoaded.delete(this.#onLoad);
    this.#onLoad = null;
  }

  #mount() {
    this.unmount();
    this.#cancelRetry();
    const generation = ++this.#generation;
    this.#mountKey = this.mountKey();
    this.#retryKey = this.retryKey();
    loadConfigEditor().then(
      () => {
        if (generation !== this.#generation || !this.element) return;
        const settings = settingsOf(this.props);
        this.#settings = settings;
        try {
          this.instance = this.create(this.element, JSON.parse(settings));
          this.failed = false;
        } catch (error) {
          this.instance = null;
          this.failed = true;
          this.report(error);
          return;
        }
        this.mounted();
      },
      (error) => {
        if (generation !== this.#generation) return;
        // When this load fails, the app's own `loadConfigEditor(url)`
        // succeeding later mounts the editor.
        this.#onLoad = () => {
          this.#cancelRetry();
          if (generation === this.#generation && this.element) this.#mount();
        };
        onLoaded.add(this.#onLoad);
        this.report(error);
      },
    );
  }
}

/** The props an `EditorBinding` reads (React-shaped; each wrapper maps its own). */
export class EditorBinding extends Binding {
  #sync = new ValueSync();
  // The `value` prop as last handled: a re-mount re-runs the value check,
  // and an unchanged prop is not a new value from the host.
  #handledValue = undefined;
  // What a re-mount (language, schema, … changed) starts from, so an
  // uncontrolled document and its diff baseline survive it.
  #carried = null;
  // Where the caret was when a re-mount took the editor away while it had
  // focus: the new one takes focus and the caret back, so typing goes on.
  #carriedFocus = null;
  // A `setValue` made while there is no editor (before the first mount, or
  // while a re-mount has failed): applied and reported on mount.
  #reset = null;
  // Each live prop as last applied.
  #applied = {};
  #followPath = (path) => this.props.onPathChange?.(path);
  #save = (value, language) => this.props.onSave?.(value, language);

  constructor(name, props) {
    super(name, props);
    /** The component's imperative API (React's ref, Vue's exposed instance, Svelte's exports). */
    this.handle = this.#handle();
  }

  mountKey() {
    const { language, schema, fileName, editorClassName, secrets } = this.props;
    // The secrets options that re-mount (the check function does not).
    const patterns = secrets
      ? [secrets.placeholder, secrets.hint, secrets.referencePrefixes, !!secrets.isPlaceholder]
      : null;
    return JSON.stringify([language ?? null, schema ?? null, fileName ?? null, editorClassName ?? null, patterns]);
  }

  retryKey() {
    return JSON.stringify(this.props.editAs ?? null);
  }

  create(element, settings) {
    const props = this.props;
    // A re-mount starts from the newest document: the editor's own, since a
    // late host's `value` may not have caught up. Values still pending stay
    // pending, so their late echoes are recognized.
    const value = this.#reset ?? this.#carried?.value ?? props.value ?? props.defaultValue ?? "";
    const baseline = props.baseline ?? this.#carried?.baseline ?? value;
    this.#carried = { value, baseline };
    return new ConfigEditor(element, {
      ...settings,
      value,
      language: props.language,
      editAs: props.editAs ?? undefined,
      schema: props.schema,
      baseline,
      fileName: props.fileName,
      readOnly: props.readOnly,
      className: props.editorClassName,
      onChange: (text) => {
        if (this.#carried) this.#carried.value = text;
        this.#sync.sent(text);
        this.props.onChange?.(text);
      },
      onEditAsChange: (language) => this.props.onEditAsChange?.(language),
      onChangeSummary: (summary) => this.props.onChangeSummary?.(summary),
      onPathChange: props.onPathChange ? this.#followPath : undefined,
      hostDiagnostics: props.hostDiagnostics,
      readOnlyPaths: props.readOnlyPaths,
      readOnlyRanges: props.readOnlyRanges,
      secrets: props.secrets && {
        ...props.secrets,
        isPlaceholder: props.secrets.isPlaceholder
          ? (token) => this.props.secrets?.isPlaceholder?.(token) ?? false
          : undefined,
      },
      onSave: props.onSave ? this.#save : undefined,
      onDirtyChange: (dirty) => this.props.onDirtyChange?.(dirty),
      onBaselineConflict: (event) => this.props.onBaselineConflict?.(event),
      onSchemaWarning: (message) => {
        const warn = this.props.onSchemaWarning;
        if (warn) warn(message);
        else console.warn(`config-editor: ${message}`);
      },
    });
  }

  mounted() {
    const focus = this.#carriedFocus;
    this.#carriedFocus = null;
    const textarea = this.element.querySelector("textarea");
    if (focus && textarea) {
      textarea.focus();
      textarea.setSelectionRange(focus.start, focus.end);
      textarea.scrollTop = focus.scrollTop;
    }
    // A reset made before there was an editor reaches the host now.
    if (this.#reset !== null) {
      const value = this.#reset;
      this.#reset = null;
      // The `value` prop still holds what the host had before the reset
      // reaches it: not a new value of its own.
      this.#handledValue = this.props.value;
      this.#sync.sent(value);
      this.props.onChange?.(value);
    }
    this.sync(true);
  }

  unmount() {
    const editor = this.instance;
    if (!editor) return;
    const textarea = this.element?.querySelector("textarea");
    if (textarea && document.activeElement === textarea) {
      this.#carriedFocus = {
        start: textarea.selectionStart,
        end: textarea.selectionEnd,
        scrollTop: textarea.scrollTop,
      };
    }
    this.instance = null;
    // destroy() first: it sends an edit still waiting to convert.
    editor.destroy();
    if (this.#carried) this.#carried.value = editor.value;
    editor.free();
  }

  /** Applies the live props that changed (all of them right after a mount). */
  sync(force) {
    const props = this.props;
    const changed = (name, key) => {
      if (!force && this.#applied[name] === key) return false;
      this.#applied[name] = key;
      return true;
    };
    const value = props.value;
    if (this.instance && value !== undefined && value !== this.#handledValue) {
      this.guard((editor) => {
        this.#handledValue = value;
        // First: an Edit-as edit still waiting out the typing pause is sent
        // now (reading `value` does), so its echo is told apart like any.
        const current = editor.value;
        const { apply, report } = this.#sync.received(value, current);
        if (apply !== null) editor.setValue(apply);
        if (report !== null) this.props.onChange?.(report);
      });
    }
    if (changed("baseline", props.baseline) && props.baseline !== undefined) {
      if (this.#carried) this.#carried.baseline = props.baseline;
      this.guard((editor) => editor.setBaseline(props.baseline));
    }
    if (changed("editAs", props.editAs ?? null)) {
      this.guard((editor) => editor.setEditAs(props.editAs ?? undefined));
    }
    if (changed("readOnly", props.readOnly ?? false)) {
      this.guard((editor) => editor.setReadOnly(props.readOnly ?? false));
    }
    const hostDiagnostics = JSON.stringify(props.hostDiagnostics ?? []);
    if (changed("hostDiagnostics", hostDiagnostics)) {
      this.guard((editor) => editor.setHostDiagnostics(JSON.parse(hostDiagnostics)));
    }
    const readOnlyPaths = JSON.stringify(props.readOnlyPaths ?? []);
    if (changed("readOnlyPaths", readOnlyPaths)) {
      this.guard((editor) => editor.setReadOnlyPaths(JSON.parse(readOnlyPaths)));
    }
    const readOnlyRanges = JSON.stringify(props.readOnlyRanges ?? []);
    if (changed("readOnlyRanges", readOnlyRanges)) {
      this.guard((editor) => editor.setReadOnlyRanges(JSON.parse(readOnlyRanges)));
    }
    const followsPath = props.onPathChange !== undefined;
    if (changed("followsPath", followsPath)) {
      this.guard((editor) => editor.setCallbacks({ onPathChange: followsPath ? this.#followPath : null }));
    }
    // `Ctrl+S` is the editor's only while `onSave` is set.
    const saves = props.onSave !== undefined;
    if (changed("saves", saves)) {
      this.guard((editor) => editor.setCallbacks({ onSave: saves ? this.#save : null }));
    }
  }

  /** The document while there is no editor: a pending reset, the carried one, the props'. */
  #document() {
    return this.#reset ?? this.#carried?.value ?? this.props.value ?? this.props.defaultValue ?? "";
  }

  #setValue(value) {
    if (typeof value !== "string") {
      this.report(new TypeError("setValue needs a string"));
      return;
    }
    // An Edit-as edit still waiting is sent first, so it is superseded with
    // the others below.
    void this.instance?.value;
    this.#sync.supersede();
    if (this.#carried) this.#carried.value = value;
    if (!this.instance) {
      this.#reset = value;
      return;
    }
    this.guard((editor) => {
      if (editor.value === value) return;
      editor.setValue(value);
      // The host's state follows the reset: it is reported like an edit,
      // and its echo is recognised like one.
      this.#sync.sent(value);
      this.props.onChange?.(value);
    });
  }

  #unifiedDiff(context) {
    if (this.instance) return this.instance.unifiedDiff(context);
    // While a re-mount has failed: the document and baseline kept for it
    // (no diff while the module has not loaded).
    const value = this.#document();
    try {
      return unifiedDiff(this.#carried?.baseline ?? this.props.baseline ?? value, value, context);
    } catch (error) {
      this.report(error);
      return "";
    }
  }

  #handle() {
    const binding = this;
    return {
      get value() {
        // While a re-mount has failed, the document kept for it.
        return binding.instance?.value ?? binding.#document();
      },
      get editor() {
        return binding.instance;
      },
      get currentPath() {
        return binding.ask((editor) => editor.currentPath, null);
      },
      get text() {
        return binding.ask((editor) => editor.text, "");
      },
      get foldedLines() {
        return binding.ask((editor) => editor.foldedLines, []);
      },
      get isDirty() {
        return binding.ask((editor) => editor.isDirty, false);
      },
      rangeOf: (path) => binding.ask((editor) => editor.rangeOf(path), null),
      pathAt: (offset) => binding.ask((editor) => editor.pathAt(offset), null),
      reveal: (path) => binding.ask((editor) => editor.reveal(path), false),
      fold: (path) => binding.ask((editor) => editor.fold(path), false),
      unfold: (path) => binding.ask((editor) => editor.unfold(path), false),
      foldAll: () => binding.guard((editor) => editor.foldAll()),
      unfoldAll: () => binding.guard((editor) => editor.unfoldAll()),
      focus: () => binding.guard((editor) => editor.focus()),
      save: () => binding.ask((editor) => editor.save(), false),
      setValue: (value) => binding.#setValue(value),
      unifiedDiff: (context) => binding.#unifiedDiff(context),
    };
  }
}

/** A `ConfigDocuments` view driven by a component's props. */
export class DocumentsBinding extends Binding {
  // Per document: what its editor holds now, and the echoes of its edits
  // still on their way through the host (see `ValueSync`).
  #texts = new Map();
  #syncs = new Map();
  #applied = {};
  #save = (id, value, language) => this.props.onSave?.(id, value, language);

  constructor(name, props) {
    super(name, props);
    /** The component's imperative API. */
    this.handle = this.#handle();
  }

  mountKey() {
    const { label, viewClassName, onClose } = this.props;
    return JSON.stringify([label ?? null, viewClassName ?? null, onClose !== undefined]);
  }

  retryKey() {
    return null;
  }

  #syncOf(id) {
    let sync = this.#syncs.get(id);
    if (!sync) this.#syncs.set(id, (sync = new ValueSync()));
    return sync;
  }

  // The host's list with late echoes of the user's own edits replaced by
  // what the editors hold, so a slow host never rolls a document back.
  #reconcile(documents) {
    const listed = new Set(documents.map((document) => document.id));
    for (const id of [...this.#texts.keys()]) {
      if (!listed.has(id)) {
        this.#texts.delete(id);
        this.#syncs.delete(id);
      }
    }
    return documents.map((document) => {
      const current = this.#texts.get(document.id);
      if (current === undefined) {
        this.#texts.set(document.id, document.value);
        return document;
      }
      const { apply, report } = this.#syncOf(document.id).received(document.value, current);
      if (report !== null) this.props.onChange?.(document.id, report);
      if (apply !== null) {
        this.#texts.set(document.id, apply);
        return document;
      }
      return current === document.value ? document : { ...document, value: current };
    });
  }

  create(element, settings) {
    const props = this.props;
    this.#texts.clear();
    this.#syncs.clear();
    return new ConfigDocuments(element, {
      ...settings,
      documents: this.#reconcile(props.documents ?? []),
      activeId: props.activeId,
      className: props.viewClassName,
      label: props.label,
      onSelect: (id) => this.props.onSelect?.(id),
      onClose: props.onClose ? (request) => this.props.onClose?.(request) : undefined,
      onChange: (id, value) => {
        this.#texts.set(id, value);
        this.#syncOf(id).sent(value);
        this.props.onChange?.(id, value);
      },
      onSave: this.#save,
      onDirtyChange: (id, dirty) => this.props.onDirtyChange?.(id, dirty),
    });
  }

  mounted() {
    this.sync(true);
  }

  unmount() {
    const view = this.instance;
    if (!view) return;
    this.instance = null;
    view.destroy();
    view.free();
  }

  sync(force) {
    const props = this.props;
    const changed = (name, key) => {
      if (!force && this.#applied[name] === key) return false;
      this.#applied[name] = key;
      return true;
    };
    const documents = props.documents ?? [];
    if (changed("documents", JSON.stringify(documents))) {
      this.guard((view) => view.setDocuments(this.#reconcile(documents)));
    }
    // `Ctrl+S` is the view's only while `onSave` is set.
    const saves = props.onSave !== undefined;
    if (changed("saves", saves)) {
      this.guard((view) => view.setCallbacks({ onSave: saves ? this.#save : null }));
    }
  }

  #handle() {
    const binding = this;
    return {
      get instance() {
        return binding.instance;
      },
      get activeId() {
        return binding.ask((view) => view.activeId ?? null, null);
      },
      select: (id) => binding.ask((view) => view.select(id), false),
      isDirty: (id) => binding.ask((view) => view.isDirty(id), false),
      save: (id) => binding.ask((view) => view.save(id), false),
      saveAll: () => binding.ask((view) => view.saveAll(), []),
      focus: () => binding.guard((view) => view.focus()),
      editor: (id) => binding.ask((view) => view.editor(id) ?? null, null),
    };
  }
}
