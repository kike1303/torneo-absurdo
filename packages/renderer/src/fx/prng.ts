// Cosmetic PRNG of the renderer (RF-A10). Never the engine's PRNG and never the fight seed
// (ADR-0002): every spectator sees the same effects because seeds derive from the cue index.

/** mulberry32: 32-bit state, values in [0, 1). */
export function mulberry32(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let z = state;
    z = Math.imul(z ^ (z >>> 15), z | 1);
    z ^= z + Math.imul(z ^ (z >>> 7), z | 61);
    return ((z ^ (z >>> 14)) >>> 0) / 4294967296;
  };
}
