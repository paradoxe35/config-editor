import type { InitInput, InitOutput } from "./pkg/config_editor.js";

/**
 * Loads the WebAssembly module, once for the page: every editor, every
 * `@pngwasi/config-editor/tools` function and every framework component
 * share it (the components call it themselves). Await it before
 * `new ConfigEditor(…)` or a tool. Pass the `.wasm` file (a URL, a
 * `Response`, bytes or a compiled module) when a bundler does not serve it
 * next to the package's JavaScript. A failed load is not kept: calling
 * again (with the right file) retries, and components whose own load failed
 * mount then.
 */
export function loadConfigEditor(
  input?: InitInput | Promise<InitInput> | { module_or_path: InitInput | Promise<InitInput> },
): Promise<InitOutput>;

/** Runs `retry` after the next load that succeeds; returns what cancels it. For the bindings. */
export function whenLoaded(retry: () => void): () => void;
