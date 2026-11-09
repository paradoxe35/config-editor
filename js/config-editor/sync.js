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
//   goes back to a value still on its way, through `replaceValue`, and
//   keeps telling the echoes on their way apart). The values sent before it
//   belong to the value it replaced (another file, a discarded draft): no
//   echoes any more, so going back to one (a file switched back to)
//   applies;
// - ranges passed with a skipped echo are in the echoed value: the binding
//   gives it as `rangesFor`, and the editor maps them to the document;
// - a skipped echo leaves the host holding an old value: unless an echo of
//   the document is still to come, the host is told the document again. A
//   value sent before one that came back may still come (out of order) or
//   never (a parent applying only the latest): it stays an echo, but is not
//   counted on to come;
// - an echo still to come may never be seen: a framework renders once for
//   several changes, so a parent storing the document's echo and then a late
//   echo of an older value in one render passes only the older one. So
//   whenever an echo leaves the host on an older value, the binding checks
//   again once a pause (`SETTLE_MS`) is over (`settle`), and a host still on
//   an older value then hears the document again. A copy of the value the
//   host holds, passed again while it is on its way, may be its echo (with a
//   later value lost before it): it counts as news once.

/** How long after an echo left the host on an older value to check it came back to the document. */
export const SETTLE_MS = 250;

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
    // `{ print, toCome, doubted, serial }`, `toCome` while no value sent
    // after it came back, `doubted` once the value the host held was passed
    // again while it was on its way, `serial` counting the values sent.
    this.pending = [];
    // The value the host passed last.
    this.lastGiven = null;
    this.serial = 0;
  }

  /**
   * Records a value the editor sends to the host (call it from `onChange`).
   * `toCome`: whether its echo is expected (a framework does not render
   * again for the value it holds already, so the echo of a value equal to
   * the bound one may never come: still an echo should it come, but not
   * counted on).
   */
  sent(value, toCome = true) {
    this.pending.push({ print: fingerprint(value), toCome, doubted: false, serial: this.serial });
    this.serial += 1;
    if (this.pending.length > this.maxPending) this.pending.shift();
  }

  /** What `received` takes as `flushed`: the values sent from now on are sent while it is handled. */
  mark() {
    return this.serial;
  }

  /**
   * Whether the host passing again the value it passed last may be news: a
   * copy of it is on its way (this may be its echo). Call `received` then.
   */
  mayEcho(value) {
    const print = fingerprint(value);
    return this.lastGiven === print && this.pending.some((sent) => sent.print === print && !sent.doubted);
  }

  /**
   * The host's value arrived. `current` is the editor's document now (read
   * `editor.value` first: it sends an edit still waiting out the typing
   * pause; `flushed`, from `mark()` before, tells the values it sent apart:
   * the host did not know them). Returns the text to apply with
   * `replaceValue`, if any, the text to report to the host through its
   * change callback, if any (already recorded as sent), and whether to
   * `settle` once a pause is over.
   * @param {string} value
   * @param {string} current
   * @param {number} [flushed]
   * @returns {{ apply: string | null, report: string | null, settle: boolean }}
   */
  received(value, current, flushed = this.serial) {
    const none = { apply: null, report: null, settle: false };
    const print = fingerprint(value);
    let news;
    if (this.lastGiven === print) {
      // Passed again: a render, or the echo of a copy sent since.
      const copy = this.pending.findLast((sent) => sent.print === print && !sent.doubted);
      if (copy) {
        copy.toCome = false;
        copy.doubted = value !== current;
      }
      news = copy?.doubted ?? false;
    } else {
      this.lastGiven = print;
      // The newest copy (a typo fixed sends the same text twice): a parent
      // applying only the latest value hands back that one.
      const at = this.pending.findLastIndex((sent) => sent.print === print);
      if (at < 0) {
        if (value === current) return none;
        // The host's own value replaces the document: the values sent
        // before belong to the one it replaced (another file, a discarded
        // draft), no echoes any more. Those sent while it was handled (an
        // edit still held, sent first) were unknown to the host: still
        // echoes.
        this.pending = this.pending.filter((sent) => sent.serial >= flushed);
        return { ...none, apply: value };
      }
      this.pending.splice(at, 1);
      for (const sent of this.pending.slice(0, at)) sent.toCome = false;
      news = true;
    }
    if (value === current) return none;
    // An echo of a value the user typed past: the host holds it now, unless
    // an echo of the document is still to come.
    const shown = fingerprint(current);
    if (this.pending.some((sent) => sent.toCome && sent.print === shown)) return { ...none, settle: news };
    this.sent(current);
    return { apply: null, report: current, settle: true };
  }

  /**
   * The pause after a `received` asking to settle is over: whether the host,
   * still on an older value, is to hear the document `current` again
   * (already recorded as sent).
   */
  settle(current) {
    if (this.lastGiven === null || this.lastGiven === fingerprint(current)) return false;
    this.sent(current);
    return true;
  }

  /** The host holds `value` (a reset it asked for): no echo to wait for. */
  given(value) {
    this.lastGiven = fingerprint(value);
  }
}
