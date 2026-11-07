<!--
  @component
  A schema-aware config editor for YAML, JSON, TOML, HCL and XML.

  ```svelte
  <ConfigEditor bind:value={text} language="yaml" {schema} onSave={upload} />
  ```

  Through `bind:this`: `reveal`, `save`, `isDirty`, `getValue`, … (the
  instance API).
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import {
    type EditorBindingProps,
    EditorBinding,
    type EditorPath,
    type EditorRange,
    type FieldPath,
    type LineRange,
  } from "@pngwasi/config-editor";
  import type { ConfigEditorProps } from "./types.js";
  import { readAll } from "./track.js";

  let {
    value = $bindable(),
    // The value as the parent passes it. Writing `value` (for `bind:value`)
    // overrides it here when the parent passes `value` one way: the editor's
    // own edit would then hide the parent's late echoes from `ValueSync`,
    // which must see what the parent has, as it has it.
    value: hostValue,
    onChange,
    editorClass,
    class: className,
    style,
    ...rest
  }: ConfigEditorProps = $props();

  let host: HTMLDivElement;
  // Everything an editor needs from a component (live props, re-mounts, the
  // bound value through `ValueSync`), shared with the React and Vue
  // packages: `EditorBinding` from `@pngwasi/config-editor`.
  const binding = new EditorBinding("config-editor-svelte");
  const { handle } = binding;

  function changed(text: string) {
    value = text;
    onChange?.(text);
  }

  // The props as the binding names them; the effect below follows each one,
  // deeply.
  function current(): EditorBindingProps {
    return { ...rest, value: hostValue, editorClassName: editorClass, onChange: changed };
  }

  onMount(() => {
    binding.attach(host, current());
    return () => binding.detach();
  });

  $effect(() => {
    // Deep: a list or object prop changed in place is a change too.
    const props = current();
    readAll(props);
    untrack(() => binding.update(props));
  });

  /** The current document, in `language`. */
  export function getValue(): string {
    return handle.value;
  }
  /** The underlying `ConfigEditor` instance, once mounted. */
  export function getEditor() {
    return handle.editor;
  }
  /** Replaces the document, always, and reports it (`value`, `onChange`); see `ConfigEditorHandle.setValue`. */
  export function setValue(text: string): void {
    handle.setValue(text);
  }
  /** The unified diff (`diff -u`) of the baseline against the document; empty when they are the same. */
  export function unifiedDiff(context?: number): string {
    return handle.unifiedDiff(context);
  }
  /** The path of what the caret is on, or `null` before the editor mounts. */
  export function getCurrentPath(): EditorPath | null {
    return handle.currentPath;
  }
  /** Where the field at `path` is written in the text on screen, or `null`. */
  export function rangeOf(path: FieldPath): EditorRange | null {
    return handle.rangeOf(path);
  }
  /** The path at a string index of the text on screen, or `null`. */
  export function pathAt(offset: number): EditorPath | null {
    return handle.pathAt(offset);
  }
  /** Scrolls to the field at `path`, selects it and focuses the editor; `false` when there is no such field. */
  export function reveal(path: FieldPath): boolean {
    return handle.reveal(path);
  }
  /** Folds the block the field at `path` opens; `false` when there is none to fold. */
  export function fold(path: FieldPath): boolean {
    return handle.fold(path);
  }
  /** Opens the folds around the field at `path`; `false` when none was closed. */
  export function unfold(path: FieldPath): boolean {
    return handle.unfold(path);
  }
  /** The text on screen (in the `editAs` language while editing as another); `""` before the editor mounts. */
  export function getText(): string {
    return handle.text;
  }
  /** The folded blocks, one-based `{ startLine, endLine }`. */
  export function getFoldedLines(): LineRange[] {
    return handle.foldedLines;
  }
  /** Folds every block. */
  export function foldAll(): void {
    handle.foldAll();
  }
  /** Opens every fold. */
  export function unfoldAll(): void {
    handle.unfoldAll();
  }
  /** Focuses the editor, keeping its selection. */
  export function focus(): void {
    handle.focus();
  }
  /** Saves as `Ctrl+S` does (calls `onSave`); `false` when nothing was sent. */
  export function save(): boolean {
    return handle.save();
  }
  /** Whether the document differs from `baseline`. */
  export function isDirty(): boolean {
    return handle.isDirty;
  }
</script>

<div bind:this={host} class={className} {style}></div>
