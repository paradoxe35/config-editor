<!--
  @component
  Several documents as tabs, one editor per document (each keeps its caret,
  undo history, scroll and folds across switches).

  ```svelte
  <ConfigDocuments documents={files} onChange={update} onSave={upload} onClose={close} />
  ```

  Through `bind:this`: `select`, `save`, `saveAll`, `editor(id)`, … (the
  instance API).
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import type { EditorView, LanguageName } from "@pngwasi/config-editor";
  import { DocumentsBinding, type DocumentsBindingProps } from "@pngwasi/config-editor/binding";
  import type { ConfigDocumentsProps } from "./types.js";
  import { readAll } from "./track.js";

  let { viewClass, class: className, style, ...rest }: ConfigDocumentsProps = $props();

  let host: HTMLDivElement;
  const binding = new DocumentsBinding("config-editor-svelte");
  const { handle } = binding;

  function current(): DocumentsBindingProps {
    return { ...rest, viewClassName: viewClass };
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

  /** The underlying `ConfigDocuments` instance, once mounted. */
  export function getInstance() {
    return handle.instance;
  }
  /** The active document's id, or `null`. */
  export function getActiveId(): string | null {
    return handle.activeId;
  }
  /** Shows the document `id` and focuses its editor; `false` when there is no such document. */
  export function select(id: string): boolean {
    return handle.select(id);
  }
  /** Whether the document `id` differs from its baseline. */
  export function isDirty(id: string): boolean {
    return handle.isDirty(id);
  }
  /** Saves the document `id` as `Ctrl+S` does (calls `onSave`); `false` when nothing was sent. */
  export function save(id: string): boolean {
    return handle.save(id);
  }
  /** Saves every document with unsaved edits; returns the ids sent. */
  export function saveAll(): string[] {
    return handle.saveAll();
  }
  /** Edits the document `id` as `language`, also when its `editAs` already says so (after an undo stepped back across the switch); `false` when there is no such document. */
  export function editAs(id: string, language: LanguageName): boolean {
    return handle.editAs(id, language);
  }
  /** Focuses the active document's editor. */
  export function focus(): void {
    handle.focus();
  }
  /** The editor of the document `id` (paths, folds, focus, saving), or `null`. */
  export function editor(id: string): EditorView | null {
    return handle.editor(id);
  }
</script>

<div bind:this={host} class={className} {style}></div>
