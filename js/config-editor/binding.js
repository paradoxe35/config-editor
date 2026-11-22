// `@pngwasi/config-editor/binding`: what a framework component does with an
// editor, written once. The React, Vue and Svelte packages are thin wrappers
// around these bindings; a package for another framework builds on them too.
//
// A binding is fed the component's props (`attach` once its element exists,
// `update` after every change, `detach` on unmount) and decides what each
// change means: settings and most props apply live (the caret, undo history
// and folds stay), the few the editor is built from re-mount it (the
// document, its baseline, the caret and the echoes still on their way
// carried over), the bound value goes to the editor's `update({ value })`
// with `echoes` on (the editor tells an echo, late or out of order, from the
// parent's own value, so none rolls the document back), a mount that failed on a bad prop is retried when a prop
// changes, and the WebAssembly loads once for every editor on the page.

import { ConfigDocuments, ConfigEditor, unifiedDiff } from "./pkg/config_editor.js";
import { loadConfigEditor, whenLoaded } from "./load.js";
import { SETTING_TYPES } from "./setting-types.js";

export { SETTING_TYPES };

/** The settings among `props`: the keys of `EditorSettings` it sets, as plain data. */
function settingsOf(props) {
  const settings = {};
  for (const key of Object.keys(SETTING_TYPES)) {
    if (props[key] !== undefined) settings[key] = props[key];
  }
  return JSON.stringify(settings);
}

/**
 * What `update` takes to go from the settings `before` to `after` (both
 * from `settingsOf`): the keys that changed, `null` for one no longer set,
 * `strings` text by text, `colors` and `chrome` token by token (`update`
 * merges them).
 */
function settingsChange(before, after) {
  const change = {};
  const same = (left, right) => JSON.stringify(left) === JSON.stringify(right);
  for (const key of Object.keys(SETTING_TYPES)) {
    if (same(before[key], after[key])) continue;
    if (SETTING_TYPES[key] === "object" && after[key]) {
      const texts = {};
      for (const text of new Set([...Object.keys(before[key] ?? {}), ...Object.keys(after[key])])) {
        texts[text] = after[key][text] ?? null;
      }
      change[key] = texts;
    } else {
      change[key] = after[key] ?? null;
    }
  }
  return change;
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
      const before = JSON.parse(this.#settings);
      this.#settings = settings;
      if (this.failed) return this.#mount();
      this.guard((instance) => instance.update(settingsChange(before, JSON.parse(settings))));
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
    if (!this.props.onError) {
      console.error(`${this.#name}:`, error);
      return;
    }
    // A handler that throws is the host's bug: it is logged, and the
    // editor stays (in every framework alike).
    try {
      this.props.onError(error);
    } catch (thrown) {
      console.error(`${this.#name}: onError threw`, thrown, "reporting", error);
    }
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
    this.#onLoad?.();
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
        this.#onLoad = whenLoaded(() => {
          this.#cancelRetry();
          if (generation === this.#generation && this.element) this.#mount();
        });
        this.report(error);
      },
    );
  }
}

/** The props an `EditorBinding` reads (React-shaped; each wrapper maps its own). */
export class EditorBinding extends Binding {
  // The `value` prop as last handed to the editor: an unchanged prop is not
  // a new value from the parent.
  #handledValue = undefined;
  #handledRevision = undefined;
  // What a re-mount (language, schema, … changed) starts from, so an
  // uncontrolled document and its diff baseline survive it.
  #carried = null;
  // Where the caret was when a re-mount took the editor away while it had
  // focus: the new one takes focus and the caret back, so typing goes on.
  #carriedFocus = null;
  // The channel of the editor a re-mount took away (`editor.channel`): the
  // new one takes it over, so the echoes still on their way to the parent
  // are told apart there too and none rolls the typing back.
  #channel = null;
  // A `setValue` (the handle's) made while there is no editor (before the first mount, or
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
    const { language, schema, schemaFormat, schemaResources, schemaOptions, fileName, editorClassName, secrets, echoKey } =
      this.props;
    // The secrets options that re-mount (the check function does not).
    const patterns = secrets
      ? [secrets.placeholder, secrets.hint, secrets.referencePrefixes, !!secrets.isPlaceholder]
      : null;
    return JSON.stringify([
      language ?? null,
      schema ?? null,
      schemaFormat ?? null,
      schemaResources ?? null,
      schemaOptions ?? null,
      fileName ?? null,
      editorClassName ?? null,
      patterns,
      echoKey ?? null,
    ]);
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
    // A list naming no text is in the parent's value. On a re-mount the
    // editor shows its own document, so the value names a list that is
    // new; the lists the editor had, rendered again, go as they are (no
    // news to the channel taken over, wherever their numbers were read).
    const listedAgain =
      this.#channel &&
      this.#applied.lists ===
        JSON.stringify([props.readOnlyRanges ?? [], props.rangesFor ?? null, props.hostDiagnostics ?? [], props.diagnosticsFor ?? null]);
    const valueNames = this.#channel && !listedAgain ? props.value : undefined;
    // The value's revision, the lists and the texts they name go as they
    // are: the editor reads each list in the text it names (on a re-mount
    // the channel taken over holds them, and the same ones rendered again
    // are no news).
    return new ConfigEditor(element, {
      ...settings,
      value,
      // The parent binds the value two ways: every `onChange` text comes
      // back as the value, maybe late and in any order.
      echoes: true,
      channel: this.#channel ?? undefined,
      // A component made new for the same document (a conditional render)
      // takes over the texts on their way of the one that is gone.
      echoKey: props.echoKey,
      language: props.language,
      editAs: props.editAs ?? undefined,
      schema: props.schema,
      schemaFormat: props.schemaFormat,
      schemaResources: props.schemaResources,
      schemaOptions: props.schemaOptions,
      baseline,
      fileName: props.fileName,
      readOnly: props.readOnly,
      readOnlyReason: props.readOnlyReason ?? undefined,
      className: props.editorClassName,
      onChange: (text, revision) => {
        if (this.#carried) this.#carried.value = text;
        this.props.onChange?.(text, revision);
      },
      onEditAsChange: (language) => this.props.onEditAsChange?.(language),
      onSyncChange: (inSync) => this.props.onSyncChange?.(inSync),
      onChangeSummary: (summary) => this.props.onChangeSummary?.(summary),
      onProblems: (problems) => this.props.onProblems?.(problems),
      onPathChange: props.onPathChange ? this.#followPath : undefined,
      valueRevision: this.#reset === null ? props.valueRevision : undefined,
      hostDiagnostics: props.hostDiagnostics,
      diagnosticsFor: props.diagnosticsFor ?? (props.rangesFor === undefined ? valueNames : undefined),
      readOnlyPaths: props.readOnlyPaths,
      readOnlyRanges: props.readOnlyRanges ?? [],
      rangesFor: props.rangesFor ?? valueNames,
      secrets: props.secrets && {
        ...props.secrets,
        isPlaceholder: props.secrets.isPlaceholder
          ? (token) => this.props.secrets?.isPlaceholder?.(token) ?? false
          : undefined,
      },
      onSave: props.onSave ? this.#save : undefined,
      onDirtyChange: (dirty) => this.props.onDirtyChange?.(dirty),
      onBaselineConflict: (event) => this.props.onBaselineConflict?.(event),
      keyNames: props.keyNames ?? [],
      onGoToDefinition: (target) => this.props.onGoToDefinition?.(target),
      onSchemaWarning: (message) => {
        const warn = this.props.onSchemaWarning;
        if (warn) warn(message);
        else console.warn(`config-editor: ${message}`);
      },
    });
  }

  mounted() {
    // The new editor took the channel over (a constructor that threw left
    // it for the next try).
    this.#channel = null;
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
      this.#handledRevision = this.props.valueRevision;
      this.props.onChange?.(value, this.instance?.sentRevision);
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
    this.#channel?.free();
    this.#channel = editor.channel;
    editor.free();
  }

  detach() {
    super.detach();
    this.#channel?.free();
    this.#channel = null;
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
    // The bound value, its revision, the lists and the texts they name go
    // in one update whenever one of them changed (the lists by JSON, only
    // to skip work): the editor decides where each list's numbers are read
    // (see the core's `host_inputs`), and a list given again is no news.
    // Right after a mount the editor has them all.
    const host = {
      value,
      valueRevision: props.valueRevision,
      readOnlyRanges: props.readOnlyRanges ?? [],
      rangesFor: props.rangesFor,
      hostDiagnostics: props.hostDiagnostics ?? [],
      diagnosticsFor: props.diagnosticsFor,
    };
    const lists = JSON.stringify([host.readOnlyRanges, host.rangesFor ?? null, host.hostDiagnostics, host.diagnosticsFor ?? null]);
    const newValue = value !== this.#handledValue || props.valueRevision !== this.#handledRevision;
    const newLists = this.#applied.lists !== lists;
    this.#applied.lists = lists;
    // Right after a mount the editor has them all, except a new value the
    // parent gave in the render that re-mounted it (the editor starts from
    // its own document): that goes as any new value does.
    const sinceMount = value !== undefined && newValue && value !== this.#carried?.value;
    if (force && !sinceMount) {
      this.#handledValue = value;
      this.#handledRevision = props.valueRevision;
    } else if (this.instance && (newLists || (value !== undefined && newValue))) {
      this.#handledValue = value;
      this.#handledRevision = props.valueRevision;
      const update = {};
      for (const [key, given] of Object.entries(host)) {
        if (given !== undefined) update[key] = given;
      }
      this.guard((editor) => editor.update(update));
    }
    // The live props that changed, each in its own `update`: a bad one
    // (reported) does not hold the others back.
    const update = {};
    if (changed("baseline", props.baseline) && props.baseline !== undefined) {
      if (this.#carried) this.#carried.baseline = props.baseline;
      update.baseline = props.baseline;
    }
    if (changed("editAs", props.editAs ?? null)) update.editAs = props.editAs ?? null;
    if (changed("readOnly", props.readOnly ?? false)) update.readOnly = props.readOnly ?? false;
    if (changed("readOnlyReason", props.readOnlyReason ?? null)) update.readOnlyReason = props.readOnlyReason ?? null;
    const paths = JSON.stringify(props.readOnlyPaths ?? []);
    if (changed("readOnlyPaths", paths) && !force) update.readOnlyPaths = JSON.parse(paths);
    const followsPath = props.onPathChange !== undefined;
    if (changed("followsPath", followsPath)) update.onPathChange = followsPath ? this.#followPath : null;
    // `Ctrl+S` is the editor's only while `onSave` is set.
    const saves = props.onSave !== undefined;
    if (changed("saves", saves)) update.onSave = saves ? this.#save : null;
    // The names by JSON: a parent builds a new list on each render.
    const keyNames = JSON.stringify(props.keyNames ?? []);
    if (changed("keyNames", keyNames) && !force) update.keyNames = JSON.parse(keyNames);
    for (const [key, given] of Object.entries(update)) {
      this.guard((editor) => editor.update({ [key]: given }));
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
    if (this.#carried) this.#carried.value = value;
    if (!this.instance) {
      this.#reset = value;
      return;
    }
    this.guard((editor) => {
      // A reset to the document as it is changes nothing, and is not reported.
      if (!editor.replaceValue(value)) return;
      // The host's state follows the reset: it is reported like an edit,
      // with the revision the editor recorded it with, so a parent keeping
      // revisions tells its echo apart exactly.
      this.props.onChange?.(value, editor.sentRevision);
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
      revealRange: (range) => binding.ask((editor) => editor.revealRange(range), false),
      fold: (path) => binding.ask((editor) => editor.fold(path), false),
      unfold: (path) => binding.ask((editor) => editor.unfold(path), false),
      foldAll: () => binding.guard((editor) => editor.foldAll()),
      unfoldAll: () => binding.guard((editor) => editor.unfoldAll()),
      focus: () => binding.guard((editor) => editor.focus()),
      save: () => binding.ask((editor) => editor.save(), false),
      editAs: (language) =>
        binding.ask((editor) => {
          editor.update({ editAs: language });
          return true;
        }, false),
      setValue: (value) => binding.#setValue(value),
      // The host's own change, reported as `setValue`'s reset is: the
      // parent's state follows from `onChange` (one way).
      applyChanges: (changes, options) =>
        binding.ask((editor) => {
          const revision = editor.applyChanges(changes, options) ?? null;
          if (revision !== null) binding.props.onChange?.(editor.value, revision);
          return revision;
        }, null),
      unifiedDiff: (context) => binding.#unifiedDiff(context),
    };
  }
}

/** A `ConfigDocuments` view driven by a component's props. The view tells
 * an echo of an edit in the list (the component lists every one back) from
 * a value of the host's own (`echoes: true`). */
export class DocumentsBinding extends Binding {
  #applied = {};
  #save = (id, value, language) => this.props.onSave?.(id, value, language);

  constructor(name, props) {
    super(name, props);
    /** The component's imperative API. */
    this.handle = this.#handle();
  }

  mountKey() {
    const { label, viewClassName, onClose, onAdd } = this.props;
    return JSON.stringify([label ?? null, viewClassName ?? null, onClose !== undefined, onAdd !== undefined]);
  }

  retryKey() {
    return null;
  }

  create(element, settings) {
    const props = this.props;
    return new ConfigDocuments(element, {
      ...settings,
      documents: props.documents ?? [],
      activeId: props.activeId,
      className: props.viewClassName,
      label: props.label,
      // The parent lists every `onChange` text back, possibly late.
      echoes: true,
      onSelect: (id) => this.props.onSelect?.(id),
      onClose: props.onClose ? (request) => this.props.onClose?.(request) : undefined,
      onAdd: props.onAdd ? () => this.props.onAdd?.() : undefined,
      onChange: (id, value, revision) => this.props.onChange?.(id, value, revision),
      onSave: this.#save,
      onDirtyChange: (id, dirty) => this.props.onDirtyChange?.(id, dirty),
      onProblems: (id, problems) => this.props.onProblems?.(id, problems),
      onEditAsChange: (id, language) => this.props.onEditAsChange?.(id, language),
      onSyncChange: (id, inSync) => this.props.onSyncChange?.(id, inSync),
      keyNames: props.keyNames ?? [],
      onGoToDefinition: (target) => this.props.onGoToDefinition?.(target),
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
    const update = {};
    // `Ctrl+S` is the view's only while `onSave` is set.
    const saves = props.onSave !== undefined;
    if (changed("saves", saves)) update.onSave = saves ? this.#save : null;
    // The list goes to the view on each render, which reads again only what
    // changed in it (a text or schema given again is not even copied): no
    // key of it here, which would serialize every document on every key.
    const documents = props.documents ?? [];
    this.guard((view) => view.update({ documents }));
    if ("onSave" in update) this.guard((view) => view.update({ onSave: update.onSave }));
    // The names by JSON: a parent builds a new list on each render.
    const keyNames = JSON.stringify(props.keyNames ?? []);
    if (changed("keyNames", keyNames) && !force) {
      this.guard((view) => view.update({ keyNames: JSON.parse(keyNames) }));
    }
    // A new `activeId` shows that document (the mount took the first one).
    const activeId = props.activeId ?? null;
    if (changed("activeId", activeId) && !force && activeId !== null) {
      this.guard((view) => view.update({ activeId }));
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
      editAs: (id, language) => binding.ask((view) => view.editAs(id, language), false),
      focus: () => binding.guard((view) => view.focus()),
      editor: (id) => binding.ask((view) => view.editor(id) ?? null, null),
      setValue: (id, value) => binding.#setValue(id, value),
      applyChanges: (id, changes, options) =>
        binding.ask((view) => {
          const revision = view.applyChanges(id, changes, options) ?? null;
          if (revision !== null) binding.props.onChange?.(id, view.editor(id).value, revision);
          return revision;
        }, null),
    };
  }

  /** Replaces a document's text, always, and reports it through `onChange`. */
  #setValue(id, value) {
    if (typeof id !== "string" || typeof value !== "string") {
      this.report(new TypeError("setValue needs an id and a string"));
      return false;
    }
    return this.ask((view) => {
      // A reset to the text the document holds changes nothing, and is not
      // reported: the parent's list holds it once.
      if (!view.replaceValue(id, value)) return false;
      // The parent's list follows, as after an edit, with the reset's revision.
      this.props.onChange?.(id, value, view.sentRevision(id));
      return true;
    }, false);
  }
}
