import { describe, expect, it } from 'vitest';
import { CHANNEL, hashSeed } from '../src/fx/hash.ts';
import { mulberry32 } from '../src/fx/prng.ts';

// RF-A10: cosmetic randomness from a local seeded PRNG.
const take = (rng: () => number, n: number) => Array.from({ length: n }, () => rng());

describe('mulberry32 and hashSeed', () => {
  it('two instances with the same seed give the same 1 000 values in [0, 1)', () => {
    const p = take(mulberry32(42), 1000);
    const q = take(mulberry32(42), 1000);
    expect(p).toEqual(q);
    expect(p.every((v) => v >= 0 && v < 1)).toBe(true);
    expect(new Set(p).size).toBeGreaterThan(990);
  });

  it('two cues, or one cue on two channels, give different sequences', () => {
    const base = take(mulberry32(hashSeed(3, CHANNEL.PARTICLES)), 8);
    expect(take(mulberry32(hashSeed(4, CHANNEL.PARTICLES)), 8)).not.toEqual(base);
    expect(take(mulberry32(hashSeed(3, CHANNEL.SHAKE)), 8)).not.toEqual(base);
    expect(hashSeed(3, CHANNEL.PARTICLES)).toBe(hashSeed(3, CHANNEL.PARTICLES));
  });

  it('matches a fixed reference vector (catches an accidental change of algorithm)', () => {
    const seed = hashSeed(0, CHANNEL.PARTICLES);
    expect(seed).toBe(REFERENCE_SEED);
    expect(take(mulberry32(seed), 5)).toEqual(REFERENCE_VALUES);
  });
});

// FNV-1a of (0, PARTICLES) and the first values of the canonical mulberry32 from that seed.
const REFERENCE_SEED = 1270166100;
const REFERENCE_VALUES = [
  0.6116843649651855, 0.2641003914177418, 0.10233899159356952, 0.16207500221207738,
  0.1977682295255363,
];
