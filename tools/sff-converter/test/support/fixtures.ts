// A minimal synthetic character with the 12 mappable animations, in SFF v1 and v2 (002 RF-B6).
// Generated in code: no MUGEN file is read, ever.
import { parseMap, type CharacterMap } from '../../src/map.ts';
import { writeAir, type AirActionSpec } from './air-writer.ts';
import { writeLz5, type Lz5Packet } from './lz5-writer.ts';
import { pcxPacketsFor, testPaletteRgb, writePcx } from './pcx-writer.ts';
import { writePng } from './png-writer.ts';
import { withSize, testPaletteV2, writeSffV2, type Sff2Sprite } from './sff2-writer.ts';
import { writeSffV1 } from './sff1-writer.ts';
import { ByteWriter } from './bytes.ts';

export const SPRITES = 6;
export const HEIGHT = 12;
export const AXIS = { x: 4, y: 11 } as const;

export function spriteWidth(k: number): number {
  return 8 + k;
}

/** Indexed pixels of sprite k: the left column is transparent (index 0), the rest 1–31. */
export function spriteIndices(k: number): number[][] {
  const w = spriteWidth(k);
  return Array.from({ length: HEIGHT }, (_, y) =>
    Array.from({ length: w }, (_, x) => (x === 0 ? 0 : 1 + ((x + y * 3 + k * 7) % 30))),
  );
}

/** RGBA of sprite k with the test palette (index i → i, 255 − i, i / 2; 0 transparent). */
export function spriteRgba(k: number): { width: number; height: number; rgba: Uint8Array } {
  const rows = spriteIndices(k);
  const width = spriteWidth(k);
  const rgba = new Uint8Array(width * HEIGHT * 4);
  rows.forEach((row, y) =>
    row.forEach((i, x) => {
      rgba.set([i, 255 - i, i >> 1, i === 0 ? 0 : 255], (y * width + x) * 4);
    }),
  );
  return { width, height: HEIGHT, rgba };
}

export const ACTIONS: readonly AirActionSpec[] = [
  {
    id: 0,
    loopStart: 1,
    frames: [
      { group: 0, image: 0, ticks: 6 },
      { group: 0, image: 1, ticks: 6 },
      { group: 0, image: 2, ticks: 6 },
    ],
  },
  {
    id: 20,
    frames: [
      { group: 0, image: 3, ticks: 5 },
      { group: 0, image: 3, ticks: 5, flip: 'H' },
    ],
  },
  {
    id: 200,
    frames: [
      { group: 0, image: 1, ticks: 3 },
      { group: 0, image: 4, x: 3, ticks: 4 },
      { group: 0, image: 0, ticks: 6 },
    ],
  },
  {
    id: 210,
    frames: [
      { group: 0, image: 2, ticks: 6 },
      { group: 0, image: 5, x: 5, ticks: 6 },
      { group: 0, image: 0, ticks: 9 },
    ],
  },
  {
    id: 1000,
    frames: [
      { group: 0, image: 4, ticks: 8 },
      { group: 0, image: 5, x: -20, y: -6, ticks: 4 },
      { group: 0, image: 1, ticks: 10 },
    ],
  },
  {
    id: 5000,
    frames: [
      { group: 0, image: 2, x: -2, ticks: 4 },
      { group: 0, image: 0, ticks: 8 },
    ],
  },
  { id: 130, frames: [{ group: 0, image: 3, ticks: 10 }] },
  {
    id: 700,
    frames: [
      { group: 0, image: 4, ticks: 6, flip: 'V' },
      { group: -1, image: 0, ticks: 3 },
      { group: 0, image: 4, ticks: 6 },
    ],
  },
  {
    id: 5150,
    frames: [
      { group: 0, image: 5, ticks: 10 },
      { group: 0, image: 5, y: 4, ticks: -1 },
      { group: 0, image: 0, ticks: 5 },
    ],
  },
  {
    id: 180,
    frames: [
      { group: 0, image: 1, ticks: 20 },
      { group: 0, image: 2, ticks: -1 },
    ],
  },
  {
    id: 190,
    frames: [
      { group: 0, image: 0, ticks: 30 },
      { group: 0, image: 3, ticks: 30, flip: 'HV' },
    ],
  },
  // Knockdown, as MUGEN splits it: 5050 falls in the air (its height comes from the physics, so
  // the AIR lifts the sprite above the axis), 5100 hits the ground and 5110 lies down.
  {
    id: 5050,
    frames: [
      { group: 0, image: 5, y: -30, ticks: 12 },
      { group: 0, image: 2, y: -20, ticks: 12 },
    ],
  },
  { id: 5100, frames: [{ group: 0, image: 4, ticks: 6 }] },
  { id: 5110, frames: [{ group: 0, image: 3, y: 2, ticks: 20 }] },
];

export const MAP_JSON = {
  id: 'local_test',
  actions: {
    attack_light: 200,
    attack_heavy: 210,
    special: 1000,
    block: 130,
    dodge: 700,
    ko: 5150,
  },
  impact_frames: { attack_light: 1, attack_heavy: 1, special: 1 },
  scale: 1,
} as const;

export function fixtureMap(): CharacterMap {
  return parseMap(JSON.parse(JSON.stringify(MAP_JSON)));
}

export function fixtureAir(): string {
  return writeAir(ACTIONS);
}

/** SFF v1, individual palettes: sprite 0 carries the palette, the rest say "same palette". */
export function fixtureSffV1(options: { shared?: boolean } = {}): Uint8Array {
  return writeSffV1({
    sharedPalette: options.shared ?? false,
    sprites: Array.from({ length: SPRITES }, (_, k) => {
      const w = spriteWidth(k);
      const bpl = w + (w % 2);
      return {
        group: 0,
        image: k,
        axisX: AXIS.x,
        axisY: AXIS.y,
        pcx: writePcx({
          width: w,
          height: HEIGHT,
          bytesPerLine: bpl,
          packets: pcxPacketsFor(spriteIndices(k), bpl),
          ...(k === 0 ? { palette: testPaletteRgb() } : {}),
        }),
        samePalette: k !== 0,
      };
    }),
  });
}

function rle8(rows: readonly (readonly number[])[]): Uint8Array {
  const w = new ByteWriter();
  const flat = rows.flat();
  for (let i = 0; i < flat.length;) {
    const v = flat[i] ?? 0;
    let n = 1;
    while (i + n < flat.length && flat[i + n] === v && n < 63) n++;
    if (n > 1 || (v & 0xc0) === 0x40) w.u8(0x40 | n).u8(v);
    else w.u8(v);
    i += n;
  }
  return w.done();
}

/** LZ5 packets: short RLE runs per row, and a long LZ copy for rows repeated from above. */
export function lz5Packets(rows: readonly (readonly number[])[]): Lz5Packet[] {
  const out: Lz5Packet[] = [];
  for (const row of rows) {
    for (let i = 0; i < row.length;) {
      const v = row[i] ?? 0;
      let n = 1;
      while (i + n < row.length && row[i + n] === v && n < 7) n++;
      out.push({ kind: 'rle', color: v, run: n });
      i += n;
    }
  }
  return out;
}

const V2_FORMATS = [0, 2, 4, 10, 11, 12] as const;

function v2Data(format: number, k: number): Uint8Array {
  const rows = spriteIndices(k);
  const width = spriteWidth(k);
  const size = width * HEIGHT;
  switch (format) {
    case 0:
      return Uint8Array.from(rows.flat());
    case 2:
      return withSize(size, rle8(rows));
    case 4:
      return withSize(size, writeLz5(lz5Packets(rows)));
    case 10:
      return withSize(
        size,
        writePng({
          width,
          height: HEIGHT,
          colorType: 3,
          pixels: Uint8Array.from(rows.flat()),
          filters: [0, 1, 2, 3, 4],
        }),
      );
    case 11: {
      const { rgba } = spriteRgba(k);
      const rgb = new Uint8Array(size * 3);
      for (let i = 0; i < size; i++) rgb.set(rgba.subarray(i * 4, i * 4 + 3), i * 3);
      return withSize(
        size,
        writePng({ width, height: HEIGHT, colorType: 2, pixels: rgb, filters: [4, 3, 2, 1, 0] }),
      );
    }
    default:
      return withSize(
        size,
        writePng({
          width,
          height: HEIGHT,
          colorType: 6,
          pixels: spriteRgba(k).rgba,
          filters: [1, 4],
        }),
      );
  }
}

/** SFF v2: one sprite per format (raw, RLE8, LZ5, PNG8, PNG24, PNG32). */
export function fixtureSpritesV2(): Sff2Sprite[] {
  return Array.from({ length: SPRITES }, (_, k) => {
    const format = V2_FORMATS[k] ?? 0;
    return {
      group: 0,
      image: k,
      width: spriteWidth(k),
      height: HEIGHT,
      axisX: AXIS.x,
      axisY: AXIS.y,
      format,
      depth: format === 11 ? 24 : format === 12 ? 32 : 8,
      data: v2Data(format, k),
      palette: 0,
      tdata: k % 2 === 1,
    };
  });
}

export function fixtureSffV2(): Uint8Array {
  return writeSffV2({
    sprites: fixtureSpritesV2(),
    palettes: [{ colors: 32, data: testPaletteV2(32) }],
  });
}
