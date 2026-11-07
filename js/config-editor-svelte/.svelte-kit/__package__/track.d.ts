/**
 * Reads every value nested in `value`, so the effect running it follows
 * changes made in place too (a document's `value` set on a `$state` list, a
 * diagnostic pushed), whatever the binding itself reads on that run.
 */
export declare function readAll(value: unknown, seen?: Set<object>): void;
