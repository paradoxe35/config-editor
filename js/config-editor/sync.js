// Keeping a `ConfigEditor` and a framework's controlled value in step.
//
// A framework binding the document two ways (React's `value` + `onChange`,
// Vue's `v-model`, Svelte's `bind:value`, Angular's ControlValueAccessor,
// Solid signals) passes every edit back in as its value, sometimes late and
// in any order (a parent applying each edit after a fetch). Applying such an
// echo would roll the document back; a value of the host's own (a reset, a
// loaded file) must be applied. A `ValueSync` tells them apart from what the
// editor sent, as a `ConfigEditor` with `echoes: true` does
// (`js/config-editor/src/echoes.rs`), so each framework wrapper only wires
// it up:
//
// - a value the editor sent that has not come back yet is an echo, whatever
//   its order or delay: skipped (and forgotten);
// - any other value is the host's own and applies (the binding's `setValue`
//   goes back to a value still on its way, through `replaceValue`);
// - a skipped echo leaves the host holding an old value: unless an echo of
//   the document is still to come, the host is told the document again. A
//   value sent before one that came back may still come (out of order) or
//   never (a parent applying only the latest): it stays an echo, but is not
//   counted on to come.

/** A short stand-in for a document: its length and a 32-bit FNV-1a hash. */
function fingerprint(text) {
  let hash = 0x811c9dc5;
  for (let index = 0; index < text.length; index += 1) {
    hash ^= text.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return `${text.length}:${hash >>> 0}`;
}

export class ValueSync {
  /**
   * @param {{ maxPending?: number }} [options] how many values sent are
   * remembered at most (a parent handing back only some leaves the others).
   */
  constructor(options = {}) {
    this.maxPending = options.maxPending ?? 256;
    // The values sent that the host has not passed back yet, oldest first:
    // `{ print, toCome }`, `toCome` while no value sent after it came back.
    this.pending = [];
  }

  /**
   * Records a value the editor sends to the host (call it from `onChange`).
   * `toCome`: whether its echo is expected (a framework does not render
   * again for the value it holds already, so the echo of a value equal to
   * the bound one may never come: still an echo should it come, but not
   * counted on).
   */
  sent(value, toCome = true) {
    this.pending.push({ print: fingerprint(value), toCome });
    if (this.pending.length > this.maxPending) this.pending.shift();
  }

  /**
   * The host's value arrived. `current` is the editor's document now (read
   * `editor.value` first: it sends an edit still waiting out the typing
   * pause). Returns the text to apply with `replaceValue`, if any, and the
   * text to report to the host through its change callback, if any (already
   * recorded as sent).
   * @param {string} value
   * @param {string} current
   * @returns {{ apply: string | null, report: string | null }}
   */
  received(value, current) {
    // The newest copy (a typo fixed sends the same text twice): a parent
    // applying only the latest value hands back that one.
    const print = fingerprint(value);
    const at = this.pending.findLastIndex((sent) => sent.print === print);
    if (at < 0) return { apply: current !== value ? value : null, report: null };
    this.pending.splice(at, 1);
    for (const sent of this.pending.slice(0, at)) sent.toCome = false;
    if (value === current) return { apply: null, report: null };
    // An echo of a value the user typed past: the host holds it now, unless
    // an echo of the document is still to come.
    const shown = fingerprint(current);
    if (this.pending.some((sent) => sent.toCome && sent.print === shown)) return { apply: null, report: null };
    this.sent(current);
    return { apply: null, report: current };
  }
}
