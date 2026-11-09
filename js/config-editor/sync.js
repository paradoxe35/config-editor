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

/** How long after a value came back the values sent before it are forgotten (see `ValueSync`). */
export const FORGET_MS = 5000;
/** A short stand-in for a document: its length and two 32-bit FNV-1a hashes (one pass). */
function fingerprint(text) {
  let hash = 0x811c9dc5;
  let other = 0x01000193;
  for (let index = 0; index < text.length; index += 1) {
    const unit = text.charCodeAt(index);
    hash = Math.imul(hash ^ unit, 0x01000193);
    other = Math.imul(other ^ unit, 0x5bd1e995);
  }
  return `${text.length}:${hash >>> 0}:${other >>> 0}`;
}

// Every value sent is kept until its echo comes, however many wait (a busy
// page, a slow parent): forgetting one would take its echo for the host's
// own value and roll the document back. It is forgotten when its echo
// comes, when the host's own value replaces the document, or `FORGET_MS`
// after a value sent later came back: a parent applying only the latest of
// several never passes the others back, and one passing them back out of
// order does so long before. So what is kept is bounded by what is sent in
// that time. A value passed back costs one read
// of it (for its fingerprint) and a lookup by it, never a pass over the
// values waiting nor another over the document.
export class ValueSync {
  // The values sent that the host has not passed back yet, by when they
  // were sent (a Map keeps that order): serial → print.
  #order = new Map();
  // The same by value: print → its copies, oldest first, each
  // `{ serial, countedOut, doubted }`: `countedOut` once its echo is not
  // counted on to come, `doubted` once the value the host held was passed
  // again while it was on its way.
  #copies = new Map();
  // When the newest of the values that came back was sent: a value sent
  // before it may still come (out of order) or never (a parent applying
  // only the latest), so it is not counted on to come.
  #cameBack = -1;
  // When each value that came back, newer than any before it, came, and
  // when it was sent: `[time, serial]`, oldest first.
  #returns = [];
  #now;
  // The value the host passed last, and the document, while known (the
  // value sent, given or applied last): prints.
  #lastGiven = null;
  #document = null;
  #documentLength = -1;
  #serial = 0;
  // The last value read, and its print (a value passed again is read once).
  #read = null;
  #readPrint = null;

  /**
   * @param {{ now?: () => number }} [options] the clock, in ms (default
   * `performance.now`).
   */
  constructor({ now = () => performance.now() } = {}) {
    this.#now = now;
  }

  /** Forgets the values sent before one that came back `FORGET_MS` ago or more. */
  #at(now) {
    let done = 0;
    while (done < this.#returns.length && this.#returns[done][0] + FORGET_MS <= now) {
      this.#forgetBefore(this.#returns[done][1]);
      done += 1;
    }
    if (done) this.#returns.splice(0, done);
  }

  /** How many values sent wait for their echo. */
  get waiting() {
    return this.#order.size;
  }

  #print(text) {
    if (text !== this.#read) {
      this.#read = text;
      this.#readPrint = fingerprint(text);
    }
    return this.#readPrint;
  }

  #documentPrint(current) {
    if (this.#document === null || this.#documentLength !== current.length) this.#shown(this.#print(current), current);
    return this.#document;
  }

  #shown(print, text) {
    this.#document = print;
    this.#documentLength = text.length;
  }

  #send(print, length, countedOut) {
    const serial = this.#serial;
    this.#serial += 1;
    this.#order.set(serial, print);
    let copies = this.#copies.get(print);
    if (!copies) this.#copies.set(print, (copies = []));
    copies.push({ serial, countedOut, doubted: false });
    this.#document = print;
    this.#documentLength = length;
  }

  /** Whether a copy of `print` is counted on to come. */
  #toCome(print) {
    const copies = this.#copies.get(print) ?? [];
    for (let at = copies.length - 1; at >= 0 && copies[at].serial > this.#cameBack; at -= 1) {
      if (!copies[at].countedOut) return true;
    }
    return false;
  }

  /** The host's own value replaced the document: the values sent before `serial` belong to the one it replaced. */
  #forgetBefore(serial) {
    for (const [sent, print] of this.#order) {
      if (sent >= serial) break;
      this.#order.delete(sent);
      // Forgotten oldest first: the value's oldest copy.
      const copies = this.#copies.get(print);
      copies.shift();
      if (copies.length === 0) this.#copies.delete(print);
    }
  }

  /**
   * Records a value the editor sends to the host (call it from `onChange`).
   * `toCome`: whether its echo is expected (a framework does not render
   * again for the value it holds already, so the echo of a value equal to
   * the bound one may never come: still an echo should it come, but not
   * counted on).
   */
  sent(value, toCome = true) {
    this.#send(this.#print(value), value.length, !toCome);
  }

  /** What `received` takes as `flushed`: the values sent from now on are sent while it is handled. */
  mark() {
    return this.#serial;
  }

  /**
   * Whether the host passing again the value it passed last may be news: a
   * copy of it is on its way (this may be its echo). Call `received` then.
   */
  mayEcho(value) {
    const print = this.#print(value);
    return this.#lastGiven === print && (this.#copies.get(print) ?? []).some((sent) => !sent.doubted);
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
  received(value, current, flushed = this.#serial) {
    const none = { apply: null, report: null, settle: false };
    const now = this.#now();
    this.#at(now);
    const print = this.#print(value);
    const copies = this.#copies.get(print);
    let news;
    if (this.#lastGiven === print) {
      // Passed again: a render, or the echo of a copy sent since.
      const copy = copies?.findLast((sent) => !sent.doubted);
      if (copy) {
        copy.countedOut = true;
        copy.doubted = value !== current;
      }
      news = copy?.doubted ?? false;
    } else {
      this.#lastGiven = print;
      if (!copies) {
        if (value === current) return none;
        // The host's own value replaces the document: the values sent
        // before belong to the one it replaced (another file, a discarded
        // draft), no echoes any more. Those sent while it was handled (an
        // edit still held, sent first) were unknown to the host: still
        // echoes.
        this.#forgetBefore(flushed);
        this.#shown(print, value);
        return { ...none, apply: value };
      }
      // The newest copy (a typo fixed sends the same text twice): a parent
      // applying only the latest value hands back that one.
      const sent = copies.pop();
      if (copies.length === 0) this.#copies.delete(print);
      this.#order.delete(sent.serial);
      if (sent.serial > this.#cameBack) {
        this.#cameBack = sent.serial;
        this.#returns.push([now, sent.serial]);
      }
      news = true;
    }
    if (value === current) {
      this.#shown(print, value);
      return none;
    }
    // An echo of a value the user typed past: the host holds it now, unless
    // an echo of the document is still to come.
    const shown = this.#documentPrint(current);
    if (this.#toCome(shown)) return { ...none, settle: news };
    this.#send(shown, current.length, false);
    return { apply: null, report: current, settle: true };
  }

  /**
   * The pause after a `received` asking to settle is over: whether the host,
   * still on an older value, is to hear the document `current` again
   * (already recorded as sent).
   */
  settle(current) {
    if (this.#lastGiven === null) return false;
    const shown = this.#documentPrint(current);
    if (this.#lastGiven === shown) return false;
    this.#send(shown, current.length, false);
    return true;
  }

  /** The host holds `value` (a reset it asked for): no echo to wait for. */
  given(value) {
    this.#lastGiven = this.#print(value);
    this.#shown(this.#lastGiven, value);
  }
}
