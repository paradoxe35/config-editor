<!-- The same page as react.html, for the same tests: the same controls and
     ids, query flags and `window` hooks. -->
<script lang="ts">
  import { onMount } from "svelte";
  import * as core from "@pngwasi/config-editor";
  import * as tools from "@pngwasi/config-editor/tools";
  import { themes } from "@pngwasi/config-editor";
  import * as api from "@pngwasi/config-editor-svelte";
  import {
    type BaselineConflict,
    type ChangeSummary,
    ConfigEditor,
    type ConfigEditorHandle,
    type ConfigEditorProps,
    type EditorPath,
    type HostDiagnostic,
  } from "@pngwasi/config-editor-svelte";

  let { schema, ready }: { schema: string; ready: Promise<unknown> } = $props();

  const query = new URLSearchParams(location.search);
  // `?uncontrolled`: `defaultValue` instead of `bind:value`. `?late`: edits
  // reach the parent 400 ms late, only the latest. `?slow`: every edit 1.5 s
  // late. `?reset-early`: `setValue` before the editor mounts.
  // `?props={...}`: extra props (`setExtraProps` changes them later).
  const uncontrolled = query.has("uncontrolled");
  const late = query.has("late");
  const slow = query.has("slow");
  const resetEarly = query.has("reset-early");
  const initialText = "# Burrow server configuration\nlisten: 7835\npublic_address: tunnel.example.com\n";

  let value = $state(initialText);
  // The version last saved on the server; saves and other clients move it.
  let baseline = $state(initialText);
  let dirty = $state(false);
  let saves: string[] = $state([]);
  let conflicts: BaselineConflict[] = $state([]);
  // The value before the latest edit, for the parent-side revert button.
  let previous = initialText;
  let lateTimer: ReturnType<typeof setTimeout> | undefined;
  let fileName = $state("burrow.yaml");
  let editAs: string | null = $state(null);
  let theme = $state("vscode-dark");
  let readOnly = $state(false);
  let editing = $state("yaml");
  let changes = $state("");
  let path = $state("");
  let hostProblems: HostDiagnostic[] = $state([]);
  let lockAddress = $state(false);
  let lockFirstLine = $state(false);
  let themeNames: string[] = $state([]);
  let shown = $state(true);
  let extra: Partial<ConfigEditorProps> = $state(JSON.parse(query.get("props") ?? "{}"));
  let editor: ReturnType<typeof ConfigEditor> | undefined = $state();

  // The instance API as a `ConfigEditorHandle` (Svelte exports functions;
  // the tests read React's ref shape).
  function asHandle(instance: ReturnType<typeof ConfigEditor>): ConfigEditorHandle {
    return {
      get value() { return instance.getValue(); },
      get editor() { return instance.getEditor(); },
      get currentPath() { return instance.getCurrentPath(); },
      get text() { return instance.getText(); },
      get foldedLines() { return instance.getFoldedLines(); },
      get isDirty() { return instance.isDirty(); },
      setValue: instance.setValue,
      unifiedDiff: instance.unifiedDiff,
      rangeOf: instance.rangeOf,
      pathAt: instance.pathAt,
      reveal: instance.reveal,
      fold: instance.fold,
      unfold: instance.unfold,
      foldAll: instance.foldAll,
      unfoldAll: instance.unfoldAll,
      focus: instance.focus,
      save: instance.save,
    };
  }

  type Change = Partial<ConfigEditorProps> | ((props: Partial<ConfigEditorProps>) => Partial<ConfigEditorProps>);
  Object.assign(window, {
    packageApi: api,
    coreApi: core,
    toolsApi: tools,
    setExtraProps: (change: Change) => (extra = typeof change === "function" ? change(extra) : change),
    editorHandle: { get current() { return editor ? asHandle(editor) : null; } },
    renameFile: () => (fileName = `renamed-${fileName}`),
    setParentValue: (text: string) => (value = text),
    setShown: (next: boolean) => (shown = next),
  });


  onMount(() => {
    ready.then(() => (themeNames = themes().map((theme) => theme.name)));
    if (resetEarly) editor?.setValue("from-effect: 1\n");
  });

  function apply(next: string) {
    previous = value;
    value = next;
  }
  // With `bind:value` the child has set `value` already: remember the one before.
  let last = initialText;
  function record(next: string) {
    previous = last;
    last = next;
  }
  function onChange(next: string) {
    if (slow) return void setTimeout(() => apply(next), 1500);
    if (!late) return apply(next);
    clearTimeout(lateTimer);
    lateTimer = setTimeout(() => apply(next), 400);
  }
  // A discard that also drops an edit the late parent has not applied yet.
  function discard(text: string) {
    clearTimeout(lateTimer);
    value = text;
    editor?.setValue(text);
  }
  function toggleHostProblems() {
    hostProblems = hostProblems.length ? [] : [{ path: "/listen", severity: "warning", message: "port in use", source: "server" }];
  }
  function onSave(text: string) {
    saves = [...saves, text];
    baseline = text;
  }
  function onChangeSummary(summary: ChangeSummary) {
    changes = `${summary.hunks} changes: +${summary.added} ~${summary.modified} -${summary.removed}`;
  }
  function onPathChange(next: EditorPath) {
    path = next.pointer || "/";
  }
  function pickEditAs(event: Event) {
    editAs = (event.target as HTMLSelectElement).value || null;
  }
  const secrets = { hint: "Keeps the stored secret" };
  const colors = { key: "#ffcc66" };
</script>

<header>
  <strong>config-editor-svelte</strong>
  <label>Edit as
    <select id="edit-as" value={editAs ?? ""} onchange={pickEditAs}>
      <option value="">Same</option><option>json</option><option>toml</option><option>hcl</option><option>xml</option>
    </select>
  </label>
  <label>Theme
    <select id="theme" bind:value={theme}>
      {#each themeNames as name (name)}<option>{name}</option>{/each}
    </select>
  </label>
  <label><input id="read-only" type="checkbox" bind:checked={readOnly} /> Read-only</label>
  <button id="replace" type="button" onclick={() => (value = "listen: 9000\n")}>Replace value</button>
  <button id="revert" type="button" onclick={() => (value = previous)}>Revert last edit</button>
  <button id="reset" type="button" onclick={() => (value = initialText)}>Reset</button>
  <button id="force-reset" type="button" onclick={() => discard(initialText)}>Reset (always)</button>
  <button id="bad-theme" type="button" onclick={() => (theme = "not-a-theme")}>Invalid theme</button>
  <button id="rename" type="button" onclick={() => (fileName = `renamed-${fileName}`)}>Rename (re-mounts)</button>
  <button id="reveal" type="button" onclick={() => editor?.reveal(["public_address"])}>Go to public_address</button>
  <button id="host-problems" type="button" onclick={toggleHostProblems}>Server problems</button>
  <label><input id="lock-address" type="checkbox" bind:checked={lockAddress} /> Lock public_address</label>
  <label><input id="lock-first-line" type="checkbox" bind:checked={lockFirstLine} /> Lock line 1</label>
  <button id="load-secret" type="button" onclick={() => (value = "clients:\n  acme:\n    secret: «secret 3f2a»\n")}>Load a secret</button>
  <button id="server-change" type="button" onclick={() => (baseline = baseline.replace("7835", "7836"))}>Change on server</button>
  <button id="save" type="button" onclick={() => editor?.save()}>Save</button>
  <span id="dirty">{dirty ? "unsaved" : "saved"}</span>
  <span id="saves" data-count={saves.length}>{saves.at(-1) ?? ""}</span>
  <span id="conflicts">{conflicts.join(",")}</span>
  <span id="editing">{editing}</span>
  <span id="changes">{changes}</span>
  <code id="breadcrumb">{path}</code>
</header>
{#snippet editorView(document: Partial<ConfigEditorProps>)}
  <ConfigEditor
    bind:this={editor}
    language="yaml"
    {schema}
    {editAs}
    onEditAsChange={(language) => (editing = language)}
    {onChangeSummary}
    {onPathChange}
    hostDiagnostics={hostProblems}
    {secrets}
    readOnlyPaths={lockAddress ? ["/public_address"] : []}
    readOnlyRanges={lockFirstLine ? [{ startLine: 1, startColumn: 1, endLine: 2, endColumn: 1 }] : []}
    {baseline}
    {onSave}
    onDirtyChange={(next) => (dirty = next)}
    onBaselineConflict={(event) => (conflicts = [...conflicts, event])}
    {theme}
    {readOnly}
    {colors}
    {fileName}
    showToolbar
    showFooter
    {...document}
    {...extra}
  />
{/snippet}
{#if !shown}
  <!-- Unmounted (`setShown(false)`). -->
{:else if uncontrolled}
  {@render editorView({ defaultValue: initialText, onChange })}
{:else if late || slow}
  <!-- A parent applying edits late: one-way `value` and `onChange`. -->
  {@render editorView({ value, onChange })}
{:else}
  <ConfigEditor
    bind:this={editor}
    bind:value
    onChange={record}
    language="yaml"
    {schema}
    {editAs}
    onEditAsChange={(language) => (editing = language)}
    {onChangeSummary}
    {onPathChange}
    hostDiagnostics={hostProblems}
    {secrets}
    readOnlyPaths={lockAddress ? ["/public_address"] : []}
    readOnlyRanges={lockFirstLine ? [{ startLine: 1, startColumn: 1, endLine: 2, endColumn: 1 }] : []}
    {baseline}
    {onSave}
    onDirtyChange={(next) => (dirty = next)}
    onBaselineConflict={(event) => (conflicts = [...conflicts, event])}
    {theme}
    {readOnly}
    {colors}
    {fileName}
    showToolbar
    showFooter
    {...extra}
  />
{/if}
<pre id="saved">{value}</pre>
