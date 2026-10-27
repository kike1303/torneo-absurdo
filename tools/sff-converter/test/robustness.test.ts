import { describe, expect, it } from 'vitest';
import { convert, type ConvertInput } from '../src/convert.ts';
import { ConverterError } from '../src/errors.ts';
import { readSff } from '../src/sff.ts';
import { elapsed } from './support/elapsed.ts';
import { fixtureAir, fixtureMap, fixtureSffV1, fixtureSffV2 } from './support/fixtures.ts';
import { mulberry32 } from './support/prng.ts';

// RF-B4: untrusted input ends well or with a ConverterError, never another exception, and each
// case takes < 1 s (measured: Vitest cannot cut a synchronous test by timeout).
const SEED = 0x5ff;
const CASES = 2000;
const MAX_MS = 1000;
const map = fixtureMap();
const air = new TextEncoder().encode(fixtureAir());
const v1 = fixtureSffV1();
const v2 = fixtureSffV2();

type Outcome = 'ok' | 'error';

function run(input: ConvertInput, label: string): Outcome {
  const result = elapsed(() => convert(input));
  if (result.error !== undefined && !(result.error instanceof ConverterError)) {
    throw new Error(`${label}: unexpected ${String(result.error)}`);
  }
  if (result.ms >= MAX_MS) throw new Error(`${label}: took ${Math.round(result.ms)} ms`);
  return result.error === undefined ? 'ok' : 'error';
}

/** End of the first sprite's data: the truncation sweep covers the header and that sprite. */
function firstSpriteEnd(bytes: Uint8Array): number {
  const file = readSff(bytes);
  const first = file.sprites[0];
  const data = first?.data;
  if (data == null) throw new Error('fixture without data');
  return data.bytes.byteOffset - bytes.byteOffset + data.length;
}

describe('robustness (RF-B4)', () => {
  for (const [name, bytes] of [
    ['v1', v1],
    ['v2', v2],
  ] as const) {
    it(`${name} truncated at every byte of the header and the first sprite`, () => {
      const end = Math.min(bytes.length - 1, firstSpriteEnd(bytes));
      for (let n = 0; n <= end; n++) {
        const outcome = run({ sff: bytes.subarray(0, n), air, map }, `${name} truncated at ${n}`);
        expect(outcome, `${name} truncated at ${n}`).toBe('error');
      }
    });
  }

  it('AIR truncated at every line still ends well or with a code', () => {
    const text = fixtureAir();
    for (let n = 0; n <= text.length; n += 7)
      run({ sff: v1, air: text.slice(0, n), map }, `air at ${n}`);
  });

  it(`${CASES} seeded mutations (seed ${SEED}) of the v1, v2 and AIR fixtures`, () => {
    const rand = mulberry32(SEED);
    const int = (n: number) => Math.floor(rand() * n);
    const counts: Record<Outcome, number> = { ok: 0, error: 0 };
    for (let i = 0; i < CASES; i++) {
      const target = i % 3;
      const source = target === 0 ? v1 : target === 1 ? v2 : air;
      let bytes = Uint8Array.from(source);
      const at = int(bytes.length);
      switch (int(6)) {
        case 0:
          bytes[at] = int(256);
          break;
        case 1: {
          const v = [0, 0xffff][int(2)] ?? 0;
          if (at + 1 < bytes.length) bytes.set([v & 0xff, v >> 8], at);
          break;
        }
        case 2: {
          const v = [0, 0xff][int(2)] ?? 0;
          if (at + 3 < bytes.length) bytes.fill(v, at, at + 4);
          break;
        }
        case 3:
          bytes = bytes.slice(0, at);
          break;
        case 4: {
          const len = 1 + int(64);
          const chunk = bytes.slice(at, at + len);
          const out = new Uint8Array(bytes.length + chunk.length);
          out.set(bytes.subarray(0, at));
          out.set(chunk, at);
          out.set(bytes.subarray(at), at + chunk.length);
          bytes = out;
          break;
        }
        default: {
          // Swap two node-sized blocks (28 bytes: an SFF v2 sprite node).
          const size = 28;
          const a = int(Math.max(1, bytes.length - size));
          const b = int(Math.max(1, bytes.length - size));
          const tmp = bytes.slice(a, a + size);
          bytes.copyWithin(a, b, b + size);
          bytes.set(tmp.subarray(0, Math.min(size, bytes.length - b)), b);
        }
      }
      const input = target === 2 ? { sff: v1, air: bytes, map } : { sff: bytes, air, map };
      counts[run(input, `seed ${SEED} case ${i}`)]++;
    }
    // Both paths are exercised (plan-b risk 4): a catch-all would make every case pass in vain.
    expect(counts.ok).toBeGreaterThanOrEqual(CASES * 0.2);
    expect(counts.error).toBeGreaterThanOrEqual(CASES * 0.2);
  });
});
