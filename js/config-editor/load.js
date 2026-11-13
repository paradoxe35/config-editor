// The one WebAssembly load, shared by every entry (`.`, `./tools`,
// `./binding`) and every framework component on the page.

import init, { ConfigDocuments, ConfigEditor } from "./pkg/config_editor.js";

// Set while a host callback runs (see `count_callback` in `src/mount.rs`).
const inHostCallback = () =>
  globalThis[Symbol.for("config-editor.inHostCallback")] === true;

// `free()` inside a callback the instance's own method fired (`save()`
// calling `onSave`, where a framework may unmount): wasm-bindgen refuses to
// free an object while one of its methods runs, so it is freed once the
// callback (and the method) has returned.
for (const Class of [ConfigEditor, ConfigDocuments]) {
  const free = Class.prototype.free;
  Class.prototype.free = function () {
    if (inHostCallback()) queueMicrotask(() => free.call(this));
    else free.call(this);
  };
}

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
 * Loads the WebAssembly module once.
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

/** Runs `retry` after the next load that succeeds; returns what cancels it. */
export function whenLoaded(retry) {
  onLoaded.add(retry);
  return () => onLoaded.delete(retry);
}
