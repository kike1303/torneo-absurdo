import { existsSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { fromHsv, parseHex, toHsv } from '../src/look/color.ts';
import { parseAccessoryManifest, parseSkinManifest } from '../src/look/manifest.ts';
import { createGarment, visibleTriangles } from '../src/look/mask.ts';
import { remapPattern } from '../src/look/pattern.ts';
import type { ColorZone } from '../src/look/types.ts';
import { islandIndex, shiftPixels, zoneTable } from '../src/look/zones.ts';

// 022: the wardrobe math, without Three. Colors are built from numbers: src/ and test/ carry no
// color literals of their own.
const hex = (r: number, g: number, b: number): string =>
  `#${[r, g, b].map((c) => c.toString(16).padStart(2, '0')).join('')}`;
const RED = hex(210, 40, 40);
const BLUE = hex(40, 60, 200);
const GREEN = hex(40, 170, 70);
const GRAY = hex(120, 120, 120);

function shift(zones: readonly ColorZone[], chosen: Record<string, string>, pixel: number[]) {
  const src = new Uint8ClampedArray([...pixel, 255]);
  const dst = new Uint8ClampedArray(4);
  shiftPixels(src, dst, [zoneTable(zones, chosen, new Set(['Body']))]);
  return [...dst];
}

describe('parseHex and hsv', () => {
  it('accepts #RRGGBB only', () => {
    expect(parseHex(hex(255, 0, 128))).toEqual([1, 0, 128 / 255]);
    for (const bad of ['red', '#fff', '#12345g', 12, undefined, null])
      expect(parseHex(bad)).toBeNull();
  });

  it('round-trips through hsv', () => {
    for (const rgb of [
      [0.2, 0.5, 0.9],
      [0.9, 0.1, 0.1],
      [0.3, 0.3, 0.3],
      [0, 0, 0],
    ] as const) {
      const [h, sat, v] = toHsv(rgb[0], rgb[1], rgb[2]);
      const out = fromHsv(h, sat, v);
      out.forEach((c, i) => expect(c).toBeCloseTo(rgb[i] ?? 0, 6));
    }
  });
});

describe('zone recolor', () => {
  const zones: ColorZone[] = [
    { id: 'z1', color: RED },
    { id: 'z2', color: BLUE },
    { id: 'z3', color: GRAY },
  ];

  it('changes nothing without a choice, or with an invalid one', () => {
    const choices: Record<string, string>[] = [{}, { z1: 'green', nope: GREEN }];
    for (const chosen of choices) {
      for (const pixel of [
        [210, 40, 40],
        [40, 60, 200],
        [9, 200, 77],
      ])
        expect(shift(zones, chosen, pixel).slice(0, 3)).toEqual(pixel);
    }
  });

  it('moves the texels of the chosen zone and leaves the other zones alone', () => {
    const chosen = { z1: GREEN };
    const [r, g, b] = shift(zones, chosen, [210, 40, 40]);
    expect(g).toBeGreaterThan((r ?? 0) + 60);
    expect(g).toBeGreaterThan((b ?? 0) + 60);
    for (const still of [
      [40, 60, 200],
      [120, 120, 120],
    ]) {
      const out = shift(zones, chosen, still);
      still.forEach((c, i) => expect(Math.abs((out[i] ?? 0) - c)).toBeLessThanOrEqual(4));
    }
  });

  it('keeps the shading: a darker texel of the zone stays darker', () => {
    const lit = shift(zones, { z1: GREEN }, [210, 40, 40]);
    const shaded = shift(zones, { z1: GREEN }, [150, 30, 30]);
    expect(Math.max(...shaded.slice(0, 3))).toBeLessThan(Math.max(...lit.slice(0, 3)) - 20);
  });

  it('a zone that follows another takes its shift', () => {
    const dark = hex(110, 20, 20);
    const following: ColorZone[] = [
      { id: 'z1', color: RED },
      { id: 'z5', color: dark, follows: 'z1' },
    ];
    const [r, g] = shift(following, { z1: GREEN }, [110, 20, 20]);
    expect(g).toBeGreaterThan(r ?? 0);
  });

  it('a zone with `only` stays put on a texture of another material', () => {
    const only: ColorZone[] = [{ id: 'z1', color: RED, only: ['Head'] }];
    expect(shift(only, { z1: GREEN }, [210, 40, 40]).slice(0, 3)).toEqual([210, 40, 40]);
  });

  it('an island keeps its color while its zone moves', () => {
    const island = { id: 'i1', zone: 'z1' };
    const names = new Set<string>();
    const tables = [
      zoneTable(zones, { z1: GREEN }, names),
      zoneTable(zones, { z1: GREEN }, names, island),
    ];
    const src = new Uint8ClampedArray([210, 40, 40, 255, 210, 40, 40, 255]);
    const dst = new Uint8ClampedArray(8);
    // The island map is gray: 0 outside, `step` on island 1.
    const map = new Uint8ClampedArray([0, 0, 0, 255, 40, 40, 40, 255]);
    shiftPixels(src, dst, tables, islandIndex(map, 40));
    expect(dst[1]).toBeGreaterThan(dst[0] ?? 0);
    expect([...dst.slice(4, 7)]).toEqual([210, 40, 40]);
  });
});

describe('pattern remap', () => {
  it('moves each texel to the same place between the new pair', () => {
    const src = new Uint8ClampedArray([200, 0, 0, 255, 250, 250, 250, 255, 225, 125, 125, 200]);
    const dst = new Uint8ClampedArray(12);
    remapPattern(
      src,
      dst,
      [
        [200 / 255, 0, 0],
        [250 / 255, 250 / 255, 250 / 255],
      ],
      [
        [0, 0, 1],
        [0, 1, 0],
      ],
    );
    expect([...dst.slice(0, 4)]).toEqual([0, 0, 255, 255]);
    expect([...dst.slice(4, 8)]).toEqual([0, 255, 0, 255]);
    expect(dst[9]).toBeGreaterThan(110);
    expect(dst[10]).toBeGreaterThan(110);
    expect(dst[11]).toBe(200);
  });
});

/** An open tube along y (no caps), as a trouser leg: radius 1, from y=0 to y=2. */
function tube(): Float32Array {
  const out: number[] = [];
  const steps = 16;
  for (let i = 0; i < steps; i++) {
    const a0 = (i / steps) * 2 * Math.PI;
    const a1 = ((i + 1) / steps) * 2 * Math.PI;
    const p = (a: number, y: number) => [Math.cos(a), y, Math.sin(a)];
    out.push(...p(a0, 0), ...p(a1, 0), ...p(a1, 2), ...p(a0, 0), ...p(a1, 2), ...p(a0, 2));
  }
  return new Float32Array(out);
}

describe('garment mask', () => {
  const garment = createGarment(tube());

  it('covers what sits inside, away from the openings', () => {
    expect(garment.covers(0, 1, 0)).toBe(true);
    expect(garment.covers(0.5, 1.5, -0.4)).toBe(true);
  });

  it('does not cover outside, past the ends or right at an opening', () => {
    expect(garment.covers(1.5, 1, 0)).toBe(false);
    expect(garment.covers(0, 2.5, 0)).toBe(false);
    expect(garment.covers(0, 0.01, 0)).toBe(false);
  });

  it('drops only the triangles with every corner covered', () => {
    const index = [0, 1, 2, 2, 3, 0];
    expect(visibleTriangles(index, new Uint8Array([1, 1, 1, 0]))).toEqual(
      new Uint32Array([2, 3, 0]),
    );
    expect(visibleTriangles(index, new Uint8Array([1, 0, 0, 0]))).toBeNull();
  });
});

describe('wardrobe manifests', () => {
  it('reads an accessory manifest and rejects a broken one', () => {
    const manifest = {
      slot: 'hands',
      attach: [{ bone: 'LeftHand' }, { bone: 'RightHand', mirror: true }],
      fit: { y: -0.1, scale: 0.95 },
      hides: false,
    };
    expect(parseAccessoryManifest(manifest)).toEqual(manifest);
    for (const bad of [
      null,
      { ...manifest, slot: 'tail' },
      { ...manifest, attach: [] },
      { ...manifest, attach: [{ bone: '' }] },
      { ...manifest, fit: { scale: 0 } },
      { ...manifest, hides: 'yes' },
    ])
      expect(parseAccessoryManifest(bad)).toBeNull();
  });

  it('reads a skin manifest and rejects a broken one', () => {
    const skin = parseSkinManifest({
      fit: { head: { scale: [1.2, 1, 1.1], offset: [0, 0.05, 0] } },
      zones: [{ id: 'z1', color: RED, label: 'ignored' }],
      islands: [{ id: 'i1', zone: 'z1', radius: 0.1 }],
      island_map: { file: 'zone-map.png', material: 'Body', step: 40 },
    });
    expect(skin).toEqual({
      fit: { head: { scale: [1.2, 1, 1.1], offset: [0, 0.05, 0] } },
      zones: { zones: [{ id: 'z1', color: RED }], islands: [{ id: 'i1', zone: 'z1' }] },
      islandMap: { file: 'zone-map.png', material: 'Body', step: 40 },
    });
    expect(parseSkinManifest({})).toEqual({});
    for (const bad of [
      [],
      { fit: { tail: { scale: [1, 1, 1], offset: [0, 0, 0] } } },
      { fit: { head: { scale: [0, 1, 1], offset: [0, 0, 0] } } },
      { zones: [{ id: 'z1', color: 'red' }] },
      { island_map: { file: '../x.png', material: 'Body', step: 40 } },
    ])
      expect(parseSkinManifest(bad)).toBeNull();
  });
});

describe('content/models/accessories', () => {
  const dir = fileURLToPath(new URL('../../../content/models/accessories/', import.meta.url));
  const catalog = JSON.parse(
    readFileSync(
      fileURLToPath(new URL('../../../content/catalog/accessories.json', import.meta.url)),
      'utf8',
    ),
  ) as { accessories: { id: string; slot: string }[] };
  /** The catalog names the slot by the garment; the body slot is where it is measured. */
  const BODY_SLOT: Record<string, string> = {
    head: 'head',
    hands: 'hands',
    feet: 'feet',
    legs: 'hips',
    torso: 'chest',
  };

  it('every folder is a catalog accessory with a valid manifest and its GLB', () => {
    const ids = readdirSync(dir, { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .map((entry) => entry.name);
    expect(ids.length).toBeGreaterThan(0);
    for (const id of ids) {
      const entry = catalog.accessories.find((accessory) => accessory.id === id);
      expect(entry, id).toBeDefined();
      const manifest = parseAccessoryManifest(
        JSON.parse(readFileSync(path.join(dir, id, 'accessory.json'), 'utf8')),
      );
      expect(manifest, id).not.toBeNull();
      expect(manifest?.slot, id).toBe(BODY_SLOT[entry?.slot ?? '']);
      expect(existsSync(path.join(dir, id, `${id}.glb`)), id).toBe(true);
    }
  });
});
