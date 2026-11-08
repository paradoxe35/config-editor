import type { PropType } from "vue";

/**
 * An event listener as Vue passes it: one function, or several when the
 * same event is listened to twice (a wrapper's own `@save` and a parent's
 * forwarded through `$attrs` are merged into an array).
 */
export type Listener<F> = F | F[];

/** A listener prop (`onSave`, …): a function or an array of them, `undefined` when not set. */
export const listener = <F>() => ({
  type: [Function, Array] as PropType<Listener<F>>,
  default: undefined,
});

type Callable = (...args: never[]) => unknown;

/** Each array of listeners already made into one function, so its identity stays while the array does. */
const joined = new WeakMap<Callable[], Callable>();

/** One function calling each listener of `listeners` in turn (the function itself when there is one). */
function join(listeners: Callable[]): Callable {
  let one = joined.get(listeners);
  if (!one) {
    const each = listeners.filter((listener) => typeof listener === "function");
    one = (...args: never[]) => {
      for (const listener of each) listener(...args);
    };
    joined.set(listeners, one);
  }
  return one;
}

/** `P` with each `on…` listener one function. */
export type Joined<P> = { [K in keyof P]: K extends `on${string}` ? Exclude<P[K], readonly unknown[]> : P[K] };

/**
 * `props` with every `on…` listener given as an array made into one
 * function calling each, as the editor binding expects one function.
 */
export function joinListeners<P extends object>(props: P): Joined<P> {
  const out: Record<string, unknown> = { ...(props as Record<string, unknown>) };
  for (const [key, value] of Object.entries(out)) {
    if (/^on[A-Z]/.test(key) && Array.isArray(value)) {
      out[key] = value.length === 0 ? undefined : join(value as Callable[]);
    }
  }
  return out as Joined<P>;
}
