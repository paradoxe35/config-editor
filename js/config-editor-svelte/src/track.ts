/**
 * Reads every value nested in `value`, so the effect running it follows
 * changes made in place too (a document's `value` set on a `$state` list, a
 * diagnostic pushed), whatever the binding itself reads on that run.
 */
export function readAll(value: unknown, seen = new Set<object>()): void {
  if (value === null || typeof value !== "object" || seen.has(value)) return;
  seen.add(value);
  for (const key in value) readAll((value as Record<string, unknown>)[key], seen);
}
