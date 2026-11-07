# @pngwasi/config-editor

A schema-aware config editor for the browser: YAML, JSON, TOML, HCL and XML,
with completion from a JSON Schema, hover docs, validation, a diff gutter,
editing one language as another, and eleven themes. It is written in Rust
([`config-editor-leptos`](https://crates.io/crates/config-editor-leptos))
and ships as WebAssembly; this package is its framework-free JavaScript API.

For React, use [`@pngwasi/config-editor-react`](https://www.npmjs.com/package/@pngwasi/config-editor-react),
which builds on this package.

## Install

```sh
npm install @pngwasi/config-editor
```

Browsers only (ES modules + WebAssembly). The `.wasm` file is about 1.5 MB
gzipped and loads once per page.

## Quick start

```ts
import init, { ConfigEditor } from "@pngwasi/config-editor";

await init();

const editor = new ConfigEditor(document.getElementById("config")!, {
  language: "yaml",
  value: "listen: 7835\n",
  schema: await (await fetch("/schema.json")).text(),
  showToolbar: true,
  showFooter: true,
  onChange: (text) => save(text),
});
```

### Loading the WebAssembly

`init()` fetches `config_editor_bg.wasm` next to the package's JavaScript,
which works with bundlers that copy it (Vite, webpack 5, Parcel). When yours
does not, pass its URL:

```js
import init from "@pngwasi/config-editor";

// Vite
import wasmUrl from "@pngwasi/config-editor/config_editor_bg.wasm?url";
await init({ module_or_path: wasmUrl });

// Any server: copy node_modules/@pngwasi/config-editor/pkg/config_editor_bg.wasm
await init({ module_or_path: "/assets/config_editor_bg.wasm" });
```

Call `init()` once, before creating editors or calling any other function.

## `new ConfigEditor(element, options)`

Mounts an editor into `element` (an `HTMLElement`; its previous content is
replaced, editors included). Options are type-checked: a value of the wrong type
throws, and an unknown option name is reported in the console.

| Option           | Type                         | Default          | Purpose                                                                                                                                                                                                       |
| ---------------- | ---------------------------- | ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `language`       | `string`                     | `"yaml"`         | Language of the document: `"yaml"`, `"json"`, `"toml"`, `"hcl"`, `"xml"` (any case, or a file extension such as `"yml"` or `"tf"`), or a registered one. `value`, `baseline` and `onChange` are always in it. |
| `value`          | `string`                     | `""`             | The document.                                                                                                                                                                                                 |
| `editAs`         | `string`                     | —                | Show and edit the document in another language; it is saved back in `language`, keeping its comments.                                                                                                         |
| `schema`         | `string`                     | —                | JSON Schema as JSON text: completion, hover docs and validation.                                                                                                                                              |
| `baseline`       | `string`                     | `value`          | Text the diff gutter compares against: the version last saved or loaded. See [Saving](#saving-and-unsaved-edits).                                                                                            |
| `theme`          | `string`                     | `"vscode-dark"`  | A theme slug; see `themes()`.                                                                                                                                                                                 |
| `colors`         | `Record<string, string>`     | —                | CSS colors overriding the theme's, by token (`bg`, `key`, `string`, `selection`, …); see `colorTokens()`. A value the browser does not read as a color throws.                                              |
| `readOnly`       | `boolean`                    | `false`          | View mode: navigation, selection, diagnostics and hover keep working.                                                                                                                                         |
| `fileName`       | `string`                     | `untitled.<ext>` | Shown in the toolbar.                                                                                                                                                                                         |
| `showToolbar`    | `boolean`                    | `false`          | File name, schema state and language.                                                                                                                                                                         |
| `showFooter`     | `boolean`                    | `false`          | Problem counts and line count.                                                                                                                                                                                |
| `search`         | `boolean`                    | `true`           | Find and replace (`Ctrl+F`, `Ctrl+H`, `F3`, as in VS Code). `false` leaves those keys to the browser.                                                                                                         |
| `goToLine`       | `boolean`                    | `true`           | Go to line (`Ctrl+G`): `line` or `line:column`. `false` leaves the key to the browser.                                                                                                                        |
| `className`      | `string`                     | —                | Extra class on the editor root.                                                                                                                                                                               |
| `onChange`       | `(value: string) => void`    | —                | Called with the document after every edit (after each typing pause while editing as another language).                                                                                                        |
| `onEditAsChange` | `(language: string) => void` | —                | Called when an `editAs` switch completes. A switch waits while the document cannot convert yet (TOML has no `null`).                                                                                          |
| `onChangeSummary` | `(summary: ChangeSummary) => void` | —          | Called with `{ added, removed, modified, hunks }`: the lines changed against `baseline` and the number of separate changes, on mount and whenever it changes.                                                  |
| `hostDiagnostics` | `HostDiagnostic[]`        | —                | Problems the host found (a server's validation), shown with the editor's own; see [Host diagnostics](#host-diagnostics).                                                                                     |
| `onPathChange`   | `(path: EditorPath) => void` | —                | Called with the path of what the caret is on each time it changes (and once on mount), for a breadcrumb; see [Paths](#paths). The path is computed only while this is set.                                  |
| `onSchemaWarning` | `(message: string) => void` | console warning | Called once per thing in `schema` the editor cannot use, such as a `patternProperties` regex its engine cannot compile (keys it matches get no completions or hover docs). |
| `onSave`         | `(value: string, language: string) => void` | — | Called on `Ctrl+S` / `Cmd+S` (the browser's dialog prevented) and `save()`, with the document in `language`. Nothing saves by itself. Without it, `Ctrl+S` is the browser's. |
| `onDirtyChange`  | `(dirty: boolean) => void`   | —                | Called with whether the document differs from `baseline`: once on mount, then each time that changes. |
| `onBaselineConflict` | `(event: BaselineConflict) => void` | — | Called when a new `baseline` arrives under unsaved edits (`"opened"`) and when that is settled (`"reloaded"`, `"kept-mine"`, `"settled"`). |

An unknown language, theme or color token, or an invalid schema, throws an
`Error` saying which.

### Instance members

| Member                                         | Description                                                                                                                                                                              |
| ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `value` (getter)                               | The current document, in `language`, including an edit made in `editAs` that is still waiting out the typing pause (it is sent to `onChange` first). Still readable after `destroy()`. |
| `setValue(text)`                               | Replace the document (for example after loading a file). The caret stays on the same text when possible.                                                                                 |
| `setBaseline(text)`                            | Set the diff baseline; pass the saved text to clear the diff.                                                                                                                            |
| `setEditAs(language \| null)`                  | Switch the editing language live; `null` edits in `language` again. The language must be able to write documents (a registered one needs `serialize`).                                   |
| `setTheme(name \| null)`                       | Switch theme; `null` returns to the default.                                                                                                                                             |
| `setColors(colors)`                            | Replace the color overrides (`{}` clears them).                                                                                                                                          |
| `setReadOnly(boolean)`                         | Toggle view mode.                                                                                                                                                                        |
| `setCallbacks({ onChange?, onEditAsChange?, onChangeSummary?, onPathChange?, onSave?, onDirtyChange?, onBaselineConflict? })` | Replace callbacks; `null` removes one, an absent key keeps it. |
| `save()`                                       | Save as `Ctrl+S` does (calls `onSave`); `false` when nothing was sent (no `onSave`, or an `editAs` edit that cannot convert back yet). |
| `isDirty` (getter)                             | Whether the document differs from the baseline. |
| `unifiedDiff(context?)`                        | The `diff -u` of the baseline against the document, with `context` unchanged lines around each change (default 3); `""` when they are the same.                                         |
| `setHostDiagnostics(list)`                     | Replace the host's problems; `[]` clears them. See [Host diagnostics](#host-diagnostics).                                                                                                 |
| `rangeOf(path)`, `pathAt(index)`, `currentPath`, `reveal(path)`, `focus()` | Paths and positions; see [Paths](#paths).                                                                                                   |
| `destroy()`                                    | Unmount. An edit still converting (while editing as another language) is sent to `onChange` first. Safe to call inside `onChange`.                                                       |
| `free()`                                       | Release the WebAssembly object after `destroy()`. Optional; the garbage collector does it too. An editor stays on the page until `destroy()`, whether or not you keep a reference to it. Call `destroy()` first: after `free()` the instance cannot be used at all (wasm-bindgen reports `null pointer passed to rust`). |

An exception thrown by a callback is logged to the console and does not
break the editor.

### Host diagnostics

When a server validates the document, show its problems with the editor's own:

```ts
editor.setHostDiagnostics([
  { path: "/web/listen", severity: "error", message: "port 80 is taken",
    code: "E_PORT", source: "burrow", docsUrl: "https://example.com/docs/ports" },
  { range: { startLine: 4, startColumn: 1, endColumn: 19 }, severity: "hint", message: "consider TLS" },
  { severity: "warning", message: "restart needed to apply" }, // the whole document
]);
```

- `path` (a JSON pointer or keys and indices) is marked where the field is
  written and follows it through edits and `editAs`. `range` is in the
  document (`value`, in `language`): string indices `{ start, end }` or
  one-based `{ startLine, startColumn, endLine?, endColumn? }`; it shows
  unplaced while editing as another language. With neither, or when the field
  is missing, the problem shows in the footer.
- `severity` is `"error"` (default), `"warning"`, `"info"` or `"hint"`;
  colors `error`, `warning`, `info` and `hint` in `colors`.
- `F8` stops at them, the footer counts them, and the hover message adds
  `source` (default `"host"`), `code` and "Open docs ↗": a click on the
  message opens `docsUrl`.
- Where the host marks the same text as one of the editor's own problems,
  only the host's shows. A new list replaces the previous one; only the
  markers update. An invalid entry throws, naming its index.

### Paths

A path names a field the same way in every language: a JSON pointer
(`"/web/hosts/1"`) or its keys and indices (`["web", "hosts", 1]`).

```ts
editor.reveal("/web/listen");     // scroll to it, select it, focus; false when absent
editor.rangeOf(["web", "listen"]);
// { start: 31, end: 43, startLine: 3, startColumn: 3, endLine: 3, endColumn: 15 } or null
editor.pathAt(35);                // { pointer: "/web/listen", segments: ["web", "listen"] } or null
editor.currentPath;               // the caret's path
new ConfigEditor(element, { onPathChange: (path) => (crumb.textContent = path.pointer || "/") });
```

`start`/`end` and `pathAt`'s index are string indices into the text on screen
(the `editAs` language while editing as another one), as `selectionStart`
counts; lines and columns are one-based. `reveal` and `rangeOf` cover the key
and its value when they share a line (`listen: 7835`, `<listen>7835</listen>`),
the key alone when the value is a block below it, and the value alone for a
list item. `pathAt` gives the key under or just before the index, else the
value or container holding it (`""` at the top level).

## Functions

| Function                            | Returns            | Description                                                                                                                                                                                                                                                      |
| ----------------------------------- | ------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `init(input?)`                      | `Promise`          | Load the WebAssembly (default export).                                                                                                                                                                                                                           |
| `validate(language, text, schema?)` | `Diagnostic[]`     | Syntax problems, plus schema problems when a schema is given.                                                                                                                                                                                                    |
| `convert(from, text, to, schema?)`  | `{ text, losses }` | Convert between languages. `losses` lists JSON pointers of values the target reads back differently. Throws when the text does not parse or the target cannot express it. With a schema, a lone XML element where it expects a list converts as a one-item list. |
| `unifiedDiff(baseline, current, context?)` | `string`    | The `diff -u` of two texts: `--- baseline` / `+++ current` headers, then `@@` hunks with `context` lines around each change (default 3). `""` when they have the same lines; line endings are compared as LF.                                                   |
| `schemaWarnings(schema)`            | `string[]`         | What the editor cannot use in a schema (the messages `onSchemaWarning` receives). Throws on an invalid schema.                                                                                                                                                   |
| `languages()`                       | `string[]`         | Built-in and registered language names.                                                                                                                                                                                                                          |
| `themes()`                          | `string[]`         | Theme slugs.                                                                                                                                                                                                                                                     |
| `colorTokens()`                     | `string[]`         | Tokens accepted by `colors`.                                                                                                                                                                                                                                     |
| `registerLanguage(definition)`      | `string`           | Add a language written in JavaScript (below); returns its name.                                                                                                                                                                                                  |

```ts
interface Diagnostic {
  line?: number; // one-based; absent when the problem has no position
  column?: number;
  endColumn?: number;
  message: string;
  severity: "error" | "warning" | "info" | "hint";
}
```

```js
import { validate, convert } from "@pngwasi/config-editor";

validate("json", '{"insecure_plaintext": "yes"}', schemaText);
// [{ line: 1, column: 24, endColumn: 29, severity: "error",
//    message: '/insecure_plaintext: "yes" is not of type "boolean"' }]

// At most 20 problems are listed; past that, a 21st without a position says
// more follow (the footer shows it too).

convert("yaml", "listen: 7835\n", "toml");
// { text: "listen = 7835\n", losses: [] }
```

## Binding to a framework

A framework that binds the document two ways (Vue's `v-model`, Svelte's
`bind:value`, Angular's `ControlValueAccessor`, Solid signals) passes every
edit back in as its value, sometimes late and several at a time. Applying
such an echo with `setValue` would roll the document back. `ValueSync` tells
the host's own values (a reset, a loaded file: applied) from echoes
(skipped). The React package is built on it.

```ts
import { ConfigEditor, ValueSync } from "@pngwasi/config-editor";

const sync = new ValueSync();
const editor = new ConfigEditor(element, {
  value,
  onChange: (text) => {
    sync.sent(text); // before telling the host
    emit(text);
  },
});

// Whenever the bound value changes:
const { apply, report } = sync.received(newValue, editor.value);
if (apply !== null) editor.setValue(apply);
if (report !== null) emit(report); // the host took a stale echo: correct it

// Replacing the document from code, whatever is in flight:
sync.supersede();
editor.setValue(text);
sync.sent(text);
emit(text);
```

A value sent within the last 10 seconds that comes back while newer ones are
still on their way is taken as a late echo
(`new ValueSync({ echoWindowMs })` changes the window). Read `editor.value`
right before `received`: it sends an edit made in `editAs` that is still
waiting out the typing pause first.

<details>
<summary><strong>Vue 3</strong>: a component for <code>v-model</code></summary>

```vue
<!-- ConfigField.vue: <ConfigField v-model="text" /> -->
<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import init, { ConfigEditor, ValueSync } from "@pngwasi/config-editor";

const model = defineModel<string>({ required: true });
const host = ref<HTMLElement>();
const sync = new ValueSync();
let editor: ConfigEditor | undefined;
let unmounted = false;

onMounted(async () => {
  await init();
  if (unmounted || !host.value) return;
  editor = new ConfigEditor(host.value, {
    language: "yaml",
    value: model.value,
    onChange: (text) => {
      sync.sent(text);
      model.value = text;
    },
  });
});

// The bound value changed: an echo of an edit, or the parent's own value.
watch(model, (value) => {
  if (!editor) return;
  const { apply, report } = sync.received(value, editor.value);
  if (apply !== null) editor.setValue(apply);
  if (report !== null) model.value = report;
});

onBeforeUnmount(() => {
  unmounted = true;
  editor?.destroy(); // sends an edit still waiting to the parent first
  editor?.free();
});
</script>

<template>
  <div ref="host" />
</template>
```

</details>

<details>
<summary><strong>Svelte 5</strong>: a component for <code>bind:value</code></summary>

```svelte
<!-- ConfigField.svelte: <ConfigField bind:value={text} /> -->
<script lang="ts">
  import { onMount } from "svelte";
  import init, { ConfigEditor, ValueSync } from "@pngwasi/config-editor";

  let { value = $bindable("") }: { value?: string } = $props();
  let host: HTMLDivElement;
  let editor: ConfigEditor | undefined = $state();
  const sync = new ValueSync();

  onMount(() => {
    let unmounted = false;
    init().then(() => {
      if (unmounted) return;
      editor = new ConfigEditor(host, {
        language: "yaml",
        value,
        onChange: (text) => {
          sync.sent(text);
          value = text;
        },
      });
    });
    return () => {
      unmounted = true;
      editor?.destroy();
      editor?.free();
    };
  });

  // The bound value changed: an echo of an edit, or the parent's own value.
  $effect(() => {
    const next = value;
    if (!editor) return;
    const { apply, report } = sync.received(next, editor.value);
    if (apply !== null) editor.setValue(apply);
    if (report !== null) value = report;
  });
</script>

<div bind:this={host}></div>
```

</details>

Angular (a `ControlValueAccessor`: `writeValue` is `received`, the
registered change function is called after `sent`) and Solid (an effect on
the value signal) follow the same three calls. Pass the other options
(`schema`, `theme`, `editAs`, …) as you would without a framework, and
update them with the setters when they change.

## Keyboard

`Ctrl+Space` suggestions · `Enter`/`Tab` accept · `Tab`/`Shift+Tab` indent ·
`Ctrl+/` comment · `Ctrl+Z`/`Ctrl+Y` undo/redo · `F8` next problem ·
`Ctrl+Enter`/`Ctrl+Shift+Enter` new line below/above · `Escape` close ·
`Alt+F5`/`Shift+Alt+F5` next/previous change against the baseline ·
`Alt+F3`/`Shift+Alt+F3` the same, opening its preview, where `Enter` or `R`
reverts the change · `Ctrl+S` save (with `onSave`).
`Cmd` works in place of `Ctrl` on macOS.

## Saving and unsaved edits

Nothing is saved by itself: `Ctrl+S` / `Cmd+S` in the editor (and
`editor.save()`) calls `onSave` with the document in `language`. While
editing as another language, an edit still waiting out the typing pause is
converted first; when it cannot be written in `language` (invalid, or a
`null` for TOML), nothing is sent, `save()` returns `false`, and the caret
goes to the problem. The document is dirty while it differs from `baseline`
(the document at mount when no `baseline` is given).

```js
const editor = new ConfigEditor(element, {
  value: loaded,
  baseline: loaded,
  onSave: async (text) => {
    await putConfig(text);
    editor.setBaseline(text); // saved: nothing is unsaved any more
  },
  onDirtyChange: (dirty) => (saveButton.disabled = !dirty),
  onBaselineConflict: (event) => console.log("baseline conflict:", event),
});
// Another client saved: the server's version moves on.
events.on("config-changed", (text) => editor.setBaseline(text));
```

A new baseline while the user has unsaved edits opens a notice in the editor,
"The file changed on the server.", focused on **Keep mine** when the user is
in the editor: **Reload** makes the new baseline the document (one undo step,
reported through `onChange`), **Keep mine** (or `Escape`) keeps the document
and diffs it against the new baseline. Without unsaved edits, or when the new
baseline is the document or a text the editor sent (`onChange`, `onSave`: a
save coming back while the user typed on), it is taken silently. A `setValue`
keeps the caret and the scroll position.

## Reviewing changes

The gutter marks added, modified and removed lines against `baseline`; a
marker opens the lines before and now with **Revert** (one undo step,
reported through `onChange`). For your own review screen:

```js
const editor = new ConfigEditor(element, {
  value: saved,
  onChangeSummary: ({ hunks }) => (reviewButton.disabled = hunks === 0),
});
reviewButton.onclick = () => showPatch(editor.unifiedDiff());
// After saving: editor.setBaseline(editor.value) clears the changes.
```

## Languages written in JavaScript

```js
import {
  registerLanguage,
  ConfigEditor,
  convert,
} from "@pngwasi/config-editor";

const env = registerLanguage({
  label: "ENV", // its name is the label lowercased: "env"
  icon: "E", // optional, toolbar icon: one or two letters or digits
  extensions: ["env"], // optional: also names the language (language: "env"), as "yml" does
  parse(text) {
    // Return a JSON value, or throw { message, line?, column? }.
    return Object.fromEntries(
      text
        .split("\n")
        .filter(Boolean)
        .map((line) => line.split("=")),
    );
  },
  serialize(value) {
    // Optional: text for a value, for conversion and editAs.
    return (
      Object.entries(value)
        .map(([key, item]) => `${key}=${item}`)
        .join("\n") + "\n"
    );
  },
  keySpan(text, pointer) {
    // Optional: { line, startColumn, endColumn } (one-based) of the key at a
    // JSON pointer, so schema problems are underlined.
  },
  valueSpan(text, pointer) {
    /* optional, like keySpan for the value */
  },
  highlight(line) {
    // Optional: [{ start, end, kind }] ranges of one line, as JavaScript
    // string indices (what `indexOf` gives); kind is
    // "key" | "string" | "value" | "literal" | "comment" | "punctuation".
  },
  // Optional syntax, as in VS Code's language configuration:
  comments: { lineComment: "#" }, // Ctrl+/ (a list: the first is written); blockComment: ["/*", "*/"] is unwrapped
  brackets: [
    ["{", "}"],
    ["[", "]"],
  ], // Enter between them opens a line
  separators: ["="], // key renames, Tab through empty values
});

new ConfigEditor(element, { language: env, schema });
convert("env", "listen=7835\n", "yaml").text; // 'listen: "7835"\n'
```

A registered language gets schema validation (placed with `keySpan` and
`valueSpan`), conversion to and from every other language, and Edit-as;
`comments`, `brackets` and `separators` add comment toggling,
bracket-aware Enter, and key/value editing. Schema completion is available
for the built-in languages only. Built-in names cannot be replaced, and a
registered language cannot take another's name or extension (`registerLanguage`
throws); registering the same label again replaces it for new editors. Its name is
the label lowercased (`"Énv"` → `"énv"`).

## Themes

`theme` and `setTheme` take a slug; `themes()` returns them all.

<details>
<summary>The eleven themes</summary>

| Slug | Name | Look |
|---|---|---|
| `vscode-dark` | VS Code Dark (the default) | dark |
| `vscode-light` | VS Code Light | light |
| `obsidian` | Obsidian Dark | dark |
| `obsidian-light` | Obsidian Light | light |
| `dracula` | Dracula | dark |
| `nord` | Nord | dark |
| `atom-one-dark` | Atom One Dark | dark |
| `github-light` | GitHub Light | light |
| `github-dark` | GitHub Dark | dark |
| `solarized-light` | Solarized Light | light |
| `vira-inspired` | Vira-inspired (original) | dark |

</details>

## Styling

The editor injects its own stylesheet. Every theme color is a CSS variable
on the `.config-editor` root, and the bundled themes set them with zero
specificity, so a plain rule in your stylesheet overrides any theme:

```css
.config-editor {
  --config-editor-bg: #101418;
  --config-editor-key: #ffcc66;
}
```

The `colors` option does the same per editor. The diff markers use `added`,
`modified` and `removed` (which follows `error` unless set), and the
footer's change count `status-changes`.

## License

MIT. See [LICENSE](LICENSE).
