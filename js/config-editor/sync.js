// Keeping a `ConfigEditor` and a framework's controlled value in step.
//
// A framework binding the document two ways (React's `value` + `onChange`,
// Vue's `v-model`, Svelte's `bind:value`, Angular's ControlValueAccessor,
// Solid signals) passes every edit back in as its value, sometimes late and
// several at a time. Applying such an echo would roll the document back; a
// value of the host's own (a reset, a loaded file) must be applied. A
// `ValueSync` tells them apart from what the editor sent, so each framework
// wrapper only wires it up.

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
   * @param {{ echoWindowMs?: number, maxPending?: number }} [options]
   */
  constructor(options = {}) {
    this.echoWindowMs = options.echoWindowMs ?? 10_000;
    this.maxPending = options.maxPending ?? 1000;
    // Values sent that the host has not passed back yet, oldest first.
    this.pending = [];
    // Values still on their way when the document was replaced.
    this.superseded = [];
  }

  /** Records a value the editor sends to the host (call it from `onChange`). */
  sent(value) {
    this.pending.push({ print: fingerprint(value), sent: Date.now() });
    if (this.pending.length > this.maxPending) this.pending.shift();
  }

  /**
   * The document was replaced from code (an imperative reset): values still
   * on their way are late echoes of what it replaced, skipped when they come.
   * Call `sent` for the new document after, when the host is told about it.
   */
  supersede() {
    // Bounded like `pending`: echoes older than the window never come.
    const now = Date.now();
    this.superseded = [...this.superseded, ...this.pending]
      .filter((echo) => now - echo.sent < this.echoWindowMs)
      .slice(-this.maxPending);
    this.pending = [];
  }

  /**
   * The host's value arrived. `current` is the editor's document now (read
   * `editor.value` first: it sends an edit still waiting out the typing
   * pause). Returns the text to apply with `update({ value })`, if any, and the text
   * to report to the host through its change callback, if any (already
   * recorded as sent).
   * @param {string} value
   * @param {string} current
   * @returns {{ apply: string | null, report: string | null }}
   */
  received(value, current) {
    const now = Date.now();
    this.pending = this.pending.filter((echo) => now - echo.sent < this.echoWindowMs);
    const print = fingerprint(value);
    // A value sent before a reset is a late echo of what it replaced.
    const replaced = this.superseded.findIndex((echo) => echo.print === print && now - echo.sent < this.echoWindowMs);
    if (replaced >= 0) {
      this.superseded.splice(0, replaced + 1);
      // The host took that old value: tell it the document again.
      if (this.superseded.length === 0 && current !== value) {
        this.sent(current);
        return { apply: null, report: current };
      }
      return { apply: null, report: null };
    }
    // Echoes come back in order, so this is the oldest pending copy (a typo
    // fixed sends the same text twice).
    const sent = this.pending.findIndex((echo) => echo.print === print);
    if (sent >= 0) {
      this.pending.splice(0, sent + 1);
      // Newer values are still on their way: this one is stale.
      if (this.pending.length > 0) return { apply: null, report: null };
    } else {
      // The host's own value: what is still on its way is superseded.
      this.supersede();
    }
    return { apply: current !== value ? value : null, report: null };
  }
}
