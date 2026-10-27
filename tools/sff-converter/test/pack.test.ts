import { describe, expect, it } from 'vitest';
import { MARGIN, pack, type PackItem } from '../src/pack.ts';
import { code } from './support/expect-error.ts';

function overlaps(a: { x: number; y: number; w: number; h: number }, b: typeof a): boolean {
  return (
    a.x < b.x + b.w + MARGIN &&
    b.x < a.x + a.w + MARGIN &&
    a.y < b.y + b.h + MARGIN &&
    b.y < a.y + a.h + MARGIN
  );
}

describe('pack (RF-B5)', () => {
  const items: PackItem[] = Array.from({ length: 40 }, (_, i) => ({
    key: `f${i}`,
    width: 10 + ((i * 7) % 23),
    height: 5 + ((i * 11) % 17),
  }));

  it('frames do not overlap (with a 1 px margin) and stay inside their sheet', () => {
    const { placements, sheets } = pack(items.slice(0, 30), 64);
    const rects = items
      .slice(0, 30)
      .map((it) => ({ ...placements.get(it.key)!, w: it.width, h: it.height }));
    for (const r of rects) {
      const sheet = sheets[r.sheet]!;
      expect(r.x + r.w).toBeLessThanOrEqual(sheet.width);
      expect(r.y + r.h).toBeLessThanOrEqual(sheet.height);
    }
    for (let i = 0; i < rects.length; i++) {
      for (let j = i + 1; j < rects.length; j++) {
        if (rects[i]!.sheet === rects[j]!.sheet) expect(overlaps(rects[i]!, rects[j]!)).toBe(false);
      }
    }
    expect(sheets.length).toBeGreaterThan(1);
  });

  it('is deterministic whatever the input order', () => {
    const a = pack(items, 128);
    const b = pack([...items].reverse(), 128);
    expect([...b.placements].sort()).toEqual([...a.placements].sort());
    expect(b.sheets).toEqual(a.sheets);
  });

  it('sheets are trimmed to what they use', () => {
    const { sheets } = pack([
      { key: 'a', width: 3, height: 4 },
      { key: 'b', width: 2, height: 2 },
    ]);
    expect(sheets).toEqual([{ width: 6, height: 4 }]);
  });

  it('more than 4 sheets is convert.too_many_sheets', () => {
    const big = Array.from({ length: 5 }, (_, i) => ({ key: `b${i}`, width: 100, height: 100 }));
    expect(code(() => pack(big, 100))).toBe('convert.too_many_sheets');
    expect(pack(big.slice(0, 4), 100).sheets).toHaveLength(4);
  });
});
