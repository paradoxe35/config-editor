// How the pages' parents hand each edit back late (`later(apply)`):
//
// - `?random`: 0-200 ms late, in whatever order the delays give (a fetch
//   per edit);
// - `?manual`: when a test says so, `releaseEchoes([3, 2])` applying the
//   edits held (numbered as they came) in one task, so the framework
//   renders once for all of them; `heldEchoes()` counts the edits held;
// - else at once.

const query = new URLSearchParams(location.search);
const random = query.has("random");
export const manual = query.has("manual");
const held: (() => void)[] = [];

const hooks = window as unknown as {
  releaseEchoes: (order: number[]) => void;
  heldEchoes: () => number;
};
hooks.releaseEchoes = (order) => order.forEach((index) => held[index]());
hooks.heldEchoes = () => held.length;

export function later(apply: () => void) {
  if (manual) held.push(apply);
  else if (random) setTimeout(apply, Math.random() * 200);
  else apply();
}
