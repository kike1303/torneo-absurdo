import { describe, expect, it } from 'vitest';
import { HUD_MIN_FONT_PX, layoutHud, type Rect } from '../src/scene/index.ts';

// RF-A13: HUD layout in CSS px, measured with fake text widths (9 px per character).
const measure = (text: string) => Array.from(text).length * 9;
const overlap = (p: Rect, q: Rect) =>
  p.x < q.x + q.width && q.x < p.x + p.width && p.y < q.y + q.height && q.y < p.y + p.height;

describe('layoutHud', () => {
  const long = 'N'.repeat(60);

  it.each([
    [375, 211],
    [1280, 720],
  ])('at %i×%i bars and names do not overlap and fonts stay ≥ 14 px', (width, height) => {
    const layout = layoutHud({ width, height }, measure, { a: long, b: long });
    expect(layout.fontPx).toBeGreaterThanOrEqual(HUD_MIN_FONT_PX);
    expect(overlap(layout.bars.a, layout.bars.b)).toBe(false);
    const nameRect = (side: 'a' | 'b'): Rect => {
      const n = layout.names[side];
      const x = n.align === 'start' ? n.x : n.x - n.width;
      return { x, y: n.y, width: n.width, height: layout.fontPx };
    };
    const a = nameRect('a');
    const b = nameRect('b');
    expect(overlap(a, b)).toBe(false);
    for (const name of [a, b]) {
      expect(overlap(name, layout.bars.a)).toBe(false);
      expect(overlap(name, layout.bars.b)).toBe(false);
      expect(name.x).toBeGreaterThanOrEqual(0);
      expect(name.x + name.width).toBeLessThanOrEqual(width);
    }
    // Cut with an ellipsis only when the full name does not fit over its bar.
    const fits = measure(long) <= layout.bars.a.width;
    expect(layout.names.a.text.endsWith('…')).toBe(!fits);
    expect(layout.names.a.width).toBeLessThanOrEqual(layout.bars.a.width);
    if (width === 375) expect(fits).toBe(false);
  });

  it('does not cut short names', () => {
    const layout = layoutHud({ width: 375, height: 211 }, measure, { a: 'Ana', b: 'Bo' });
    expect(layout.names.a.text).toBe('Ana');
    expect(layout.names.b.text).toBe('Bo');
  });

  it('cuts by code points, never splitting an emoji', () => {
    const layout = layoutHud({ width: 375, height: 211 }, measure, {
      a: '\u{1F600}'.repeat(40),
      b: 'x',
    });
    expect(layout.names.a.text).toMatch(/^(\u{1F600})+…$/u);
  });
});
