// Recolor by zones (022): a body with one baked texture changes color zone by zone. Every texel
// moves by how much it belongs to each zone (a soft weight around the zone's mean color), so the
// shading and the detail stay. The shifts are tabulated over a coarse RGB grid.
import { fromHsv, parseHex, toHsv } from './color.ts';
import type { ColorChoice, ColorZone, ZoneIsland } from './types.ts';

/** Cells per channel of the shift table. */
export const LUT_SIZE = 64;
/** How much brightness counts against hue and saturation when weighing a texel. */
const VALUE_WEIGHT = 0.6;
/** Width of a zone around its mean color. */
const ZONE_SIGMA = 0.1;
/** Brightness floor of the ratios, so a black zone does not divide by zero. */
const MIN_VALUE = 0.03;

interface Anchor {
  readonly x: number;
  readonly y: number;
  readonly v: number;
}

function anchorOf(rgb: readonly [number, number, number]): Anchor {
  const [h, s, v] = toHsv(rgb[0], rgb[1], rgb[2]);
  return { x: s * Math.cos(2 * Math.PI * h), y: s * Math.sin(2 * Math.PI * h), v };
}

/** The shift of each RGB cell, worked out the first time a texel falls in it. */
export interface ZoneTable {
  /** `LUT_SIZE³` triples; a cell is valid once `shiftAt` has been asked for it. */
  readonly data: Float32Array;
  /** Index in `data` of the shift of cell `cell`, filled if it was not. */
  shiftAt(cell: number): number;
}

/**
 * The shifts of a texture. `materials` are the ones that draw it (for `only`). With `island`, its
 * zone takes the island's own color: unchanged unless the choice names the island. A baked texture
 * uses a small part of the RGB cube, so the cells are filled on demand.
 */
export function zoneTable(
  zones: readonly ColorZone[],
  chosen: ColorChoice,
  materials: ReadonlySet<string>,
  island?: ZoneIsland,
): ZoneTable {
  const moves = zones.map((zone) => {
    const from = anchorOf(parseHex(zone.color) ?? [0, 0, 0]);
    const pick = (id: string): Anchor => {
      const color = parseHex(chosen[id]);
      return color === null ? from : anchorOf(color);
    };
    if (zone.only !== undefined && !zone.only.some((name) => materials.has(name)))
      return { from, to: from };
    if (island !== undefined && island.zone === zone.id) return { from, to: pick(island.id) };
    return { from, to: pick(zone.id) };
  });
  zones.forEach((zone, i) => {
    const lead = moves[zones.findIndex((other) => other.id === zone.follows)];
    const own = moves[i];
    if (lead === undefined || own === undefined || zone.follows === undefined) return;
    if (lead.from === lead.to) return;
    const { from } = own;
    moves[i] = {
      from,
      to: {
        x: from.x + lead.to.x - lead.from.x,
        y: from.y + lead.to.y - lead.from.y,
        v: (from.v * lead.to.v) / Math.max(lead.from.v, MIN_VALUE),
      },
    };
  });
  const still = moves.every(({ from, to }) => from === to);
  const data = new Float32Array(LUT_SIZE * LUT_SIZE * LUT_SIZE * 3);
  const ready = new Uint8Array(LUT_SIZE * LUT_SIZE * LUT_SIZE);

  const fill = (cell: number): void => {
    ready[cell] = 1;
    if (still) return;
    const r = (Math.floor(cell / (LUT_SIZE * LUT_SIZE)) + 0.5) / LUT_SIZE;
    const g = ((Math.floor(cell / LUT_SIZE) % LUT_SIZE) + 0.5) / LUT_SIZE;
    const b = ((cell % LUT_SIZE) + 0.5) / LUT_SIZE;
    const [h, s, v] = toHsv(r, g, b);
    const x = s * Math.cos(2 * Math.PI * h);
    const y = s * Math.sin(2 * Math.PI * h);
    let total = 1e-9;
    let dx = 0;
    let dy = 0;
    let dv = 0;
    // Every zone weighs in, moved or not: a still zone holds its texels in place.
    for (const { from, to } of moves) {
      const d2 = (x - from.x) ** 2 + (y - from.y) ** 2 + (VALUE_WEIGHT * (v - from.v)) ** 2;
      const w = Math.exp(-d2 / ZONE_SIGMA ** 2);
      total += w;
      if (from === to) continue;
      dx += w * (to.x - from.x);
      dy += w * (to.y - from.y);
      dv += w * Math.log(Math.max(to.v, MIN_VALUE) / Math.max(from.v, MIN_VALUE));
    }
    const nx = x + dx / total;
    const ny = y + dy / total;
    const out = fromHsv(
      (Math.atan2(ny, nx) / (2 * Math.PI) + 1) % 1,
      Math.min(1, Math.hypot(nx, ny)),
      Math.min(1, v * Math.exp(dv / total)),
    );
    data[cell * 3] = out[0] - r;
    data[cell * 3 + 1] = out[1] - g;
    data[cell * 3 + 2] = out[2] - b;
  };

  return {
    data,
    shiftAt(cell) {
      if (ready[cell] !== 1) fill(cell);
      return cell * 3;
    },
  };
}

/**
 * Writes `src` (RGBA bytes) shifted into `dst`. `tables[0]` is the plain one; with `islandOf` (one
 * island number per texel, 0 for none), texel `i` uses `tables[islandOf[i]]`.
 */
export function shiftPixels(
  src: Uint8ClampedArray,
  dst: Uint8ClampedArray,
  tables: readonly ZoneTable[],
  islandOf?: Uint8Array,
): void {
  const plain = tables[0];
  if (plain === undefined) return;
  for (let i = 0; i < src.length; i += 4) {
    const table = (islandOf === undefined ? plain : tables[islandOf[i >> 2] ?? 0]) ?? plain;
    const r = src[i] ?? 0;
    const g = src[i + 1] ?? 0;
    const b = src[i + 2] ?? 0;
    const n = table.shiftAt(
      (((r * LUT_SIZE) >> 8) * LUT_SIZE + ((g * LUT_SIZE) >> 8)) * LUT_SIZE + ((b * LUT_SIZE) >> 8),
    );
    const { data } = table;
    dst[i] = r + (data[n] ?? 0) * 255;
    dst[i + 1] = g + (data[n + 1] ?? 0) * 255;
    dst[i + 2] = b + (data[n + 2] ?? 0) * 255;
    dst[i + 3] = src[i + 3] ?? 255;
  }
}

/** The island number of each texel, from the gray of the island map (`step` per island). */
export function islandIndex(gray: Uint8ClampedArray, step: number): Uint8Array {
  const out = new Uint8Array(gray.length / 4);
  for (let i = 0; i < out.length; i++) out[i] = Math.round((gray[i * 4] ?? 0) / step);
  return out;
}
