<!-- The Svelte README "Saving" example as written, type-checked with no warnings (`npm run typecheck`). -->
<script lang="ts" module>
  declare function putConfig(text: string): Promise<void>;
</script>

<script lang="ts">
  import { untrack } from "svelte";
  import { ConfigEditor } from "@pngwasi/config-editor-svelte";

  let { loaded }: { loaded: string } = $props();
  // The file as loaded, read once: the editor's state starts from it.
  let saved = $state(untrack(() => loaded));
  let text = $state(untrack(() => loaded));
  let dirty = $state(false);
</script>

<ConfigEditor
  bind:value={text}
  baseline={saved}
  onSave={async (next) => {
    await putConfig(next);
    saved = next; // nothing unsaved any more
  }}
  onDirtyChange={(next) => (dirty = next)}
/>
<span>{dirty}</span>
