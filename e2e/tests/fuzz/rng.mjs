// A small seeded random source (mulberry32): the same seed gives the same
// sequence of actions on every machine.
export function rng(seed) {
  let state = seed >>> 0;
  const next = () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  const int = (low, high) => low + Math.floor(next() * (high - low + 1));
  const pick = (items) => items[Math.floor(next() * items.length)];
  // `weights`: [[weight, value], ...].
  const weighted = (weights) => {
    const total = weights.reduce((sum, [weight]) => sum + weight, 0);
    let at = next() * total;
    for (const [weight, value] of weights) {
      at -= weight;
      if (at < 0) return value;
    }
    return weights[weights.length - 1][1];
  };
  return { next, int, pick, weighted, chance: (p) => next() < p };
}
