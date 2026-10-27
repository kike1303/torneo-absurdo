// Seeds for cosmetic effects: FNV-1a (32 bits) over the little-endian bytes of two integers
// (RF-A10). Same cue and channel, same seed, on every client.

/** One channel per effect, so two effects of the same cue never share a sequence. */
export const CHANNEL = Object.freeze({
  PARTICLES: 1,
  SHAKE: 2,
  NUMBERS: 3,
});
export type Channel = (typeof CHANNEL)[keyof typeof CHANNEL];

const FNV_OFFSET = 2166136261;
const FNV_PRIME = 16777619;

function mix(hash: number, value: number): number {
  let h = hash;
  const v = value >>> 0;
  for (let shift = 0; shift < 32; shift += 8) {
    h ^= (v >>> shift) & 255;
    h = Math.imul(h, FNV_PRIME) >>> 0;
  }
  return h;
}

export function hashSeed(cueIndex: number, channel: Channel): number {
  return mix(mix(FNV_OFFSET, cueIndex), channel);
}
