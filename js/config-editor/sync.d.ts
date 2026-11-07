/**
 * Keeps a `ConfigEditor` and a framework's controlled (two-way bound) value
 * in step: tells the host's own values (applied) from late echoes of what
 * the editor sent (skipped, so the document never rolls back).
 *
 * ```ts
 * const sync = new ValueSync();
 * const editor = new ConfigEditor(element, {
 *   value,
 *   onChange: (text) => { sync.sent(text); emit(text); },
 * });
 * // When the bound value changes:
 * const { apply, report } = sync.received(newValue, editor.value);
 * if (apply !== null) editor.setValue(apply);
 * if (report !== null) emit(report);
 * // An imperative reset:
 * sync.supersede(); editor.setValue(text); sync.sent(text); emit(text);
 * ```
 */
export class ValueSync {
  /**
   * `echoWindowMs`: how long a sent value can come back as a late echo
   * (default 10 000); `maxPending`: values remembered (default 1000).
   */
  constructor(options?: { echoWindowMs?: number; maxPending?: number });
  /** Records a value the editor sends to the host (call it from `onChange`). */
  sent(value: string): void;
  /**
   * The document was replaced from code: values still on their way are late
   * echoes of what it replaced. Call `sent` for the new document after.
   */
  supersede(): void;
  /**
   * The host's value arrived; `current` is `editor.value` read just before.
   * `apply`: the text to `setValue`, if any. `report`: the text to send the
   * host through its change callback, if any (already recorded as sent).
   */
  received(value: string, current: string): { apply: string | null; report: string | null };
}
