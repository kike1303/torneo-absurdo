import { describe, expect, it } from 'vitest';
import { dedupe, fnv1a, pack } from '../src/pack.ts';

// 007 RF-B2 and plan-b decision 13: deduplication and the grid.
const CELL = 192;
const SIZE = CELL * CELL * 4;

/** A cell whose pixel `n` (inside the cell) is opaque: distinct for every n. */
function cell(n: number): Uint8Array {
  const c = new Uint8Array(SIZE);
  c[n * 4 + 3] = 255;
  return c;
}

describe('fnv1a', () => {
  it('is the 32-bit FNV-1a', () => {
    expect(fnv1a(new Uint8Array())).toBe(0x811c9dc5);
    expect(fnv1a(new TextEncoder().encode('a'))).toBe(0xe40c292c);
  });
});

describe('dedupe', () => {
  it('keeps the first index of each content', () => {
    const { unique, indexOf } = dedupe([cell(1), cell(2), cell(1), cell(3), cell(2)]);
    expect(unique).toHaveLength(3);
    expect(indexOf).toEqual([0, 1, 0, 2, 1]);
  });
});

describe('pack', () => {
  it('lays out 10 columns per row and opens a sheet every 100 frames', () => {
    const result = pack(Array.from({ length: 101 }, (_, i) => cell(i)));
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const { sheets, frames } = result.packed;
    expect(sheets.map((s) => [s.width, s.height])).toEqual([
      [1920, 1920],
      [192, 192],
    ]);
    expect(frames[0]).toEqual({ sheet: 0, x: 0, y: 0 });
    expect(frames[11]).toEqual({ sheet: 0, x: 192, y: 192 });
    expect(frames[99]).toEqual({ sheet: 0, x: 1728, y: 1728 });
    expect(frames[100]).toEqual({ sheet: 1, x: 0, y: 0 });
    // The pixel of cell 11 landed in its place.
    const s0 = sheets[0];
    expect(s0?.data[((192 + 0) * 1920 + 192 + 11) * 4 + 3]).toBe(255);
  });

  it('uses the width of the frames in a short sheet', () => {
    const result = pack([cell(0), cell(1), cell(2)]);
    expect(result.ok && result.packed.sheets.map((s) => [s.width, s.height])).toEqual([[576, 192]]);
  });

  it('fails with more unique frames than sheetsMax full sheets', () => {
    expect(pack(Array.from({ length: 401 }, (_, i) => cell(i)))).toEqual({
      ok: false,
      frames: 401,
      max: 400,
    });
    expect(pack(Array.from({ length: 401 }, (_, i) => cell(i % 400))).ok).toBe(true);
  });
});
