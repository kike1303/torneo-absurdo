// Deterministic packing (007 RF-B2, plan-b decision 13): frames in canonical order, deduplicated
// by content (FNV-1a to group, byte comparison to confirm; the first frame with a content fixes
// its index), then laid out in a grid of TOOL_LIMITS.columns × TOOL_LIMITS.rows cells per sheet,
// without margins (guide.frame_clipped keeps the edge of every cell transparent).
import { GUIDE_LIMITS, type RgbaImage } from '@torneo/renderer/guide';
import { LIMITS } from '@torneo/renderer/manifest';
import { TOOL_LIMITS } from './limits.ts';

export interface PackedFrame {
  readonly sheet: number;
  readonly x: number;
  readonly y: number;
}

export interface Packed {
  /** Unique contents, in order of first appearance. */
  readonly unique: readonly Uint8Array[];
  /** For every input frame, the index of its unique content. */
  readonly indexOf: readonly number[];
  readonly frames: readonly PackedFrame[];
  readonly sheets: readonly RgbaImage[];
}

export type PackResult =
  | { readonly ok: true; readonly packed: Packed }
  | { readonly ok: false; readonly frames: number; readonly max: number };

/** FNV-1a, 32 bits (not node:crypto, which the lint rules forbid in the tool). */
export function fnv1a(bytes: Uint8Array): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < bytes.length; i++) {
    hash ^= bytes[i] ?? 0;
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash;
}

function equal(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

export function dedupe(cells: readonly Uint8Array[]): {
  unique: Uint8Array[];
  indexOf: number[];
} {
  const unique: Uint8Array[] = [];
  const indexOf: number[] = [];
  const byHash = new Map<number, number[]>();
  for (const cell of cells) {
    const hash = fnv1a(cell);
    const bucket = byHash.get(hash) ?? [];
    let index = bucket.find((i) => equal(unique[i] as Uint8Array, cell));
    if (index === undefined) {
      index = unique.length;
      unique.push(cell);
      bucket.push(index);
      byHash.set(hash, bucket);
    }
    indexOf.push(index);
  }
  return { unique, indexOf };
}

/** Packs cells of `cellPx × cellPx` RGBA; more than `sheetsMax` full sheets fail. */
export function pack(cells: readonly Uint8Array[]): PackResult {
  const cell = GUIDE_LIMITS.cellPx;
  const perSheet = TOOL_LIMITS.columns * TOOL_LIMITS.rows;
  const max = LIMITS.sheetsMax * perSheet;
  const { unique, indexOf } = dedupe(cells);
  if (unique.length > max) return { ok: false, frames: unique.length, max };
  const sheetCount = Math.ceil(unique.length / perSheet);
  const sheets: RgbaImage[] = [];
  for (let k = 0; k < sheetCount; k++) {
    const n = Math.min(perSheet, unique.length - k * perSheet);
    const width = Math.min(n, TOOL_LIMITS.columns) * cell;
    const height = Math.ceil(n / TOOL_LIMITS.columns) * cell;
    sheets.push({ width, height, data: new Uint8Array(width * height * 4) });
  }
  const frames: PackedFrame[] = unique.map((rgba, i) => {
    const sheet = Math.floor(i / perSheet);
    const j = i % perSheet;
    const x = (j % TOOL_LIMITS.columns) * cell;
    const y = Math.floor(j / TOOL_LIMITS.columns) * cell;
    const image = sheets[sheet] as RgbaImage;
    for (let row = 0; row < cell; row++) {
      const from = row * cell * 4;
      image.data.set(rgba.subarray(from, from + cell * 4), ((y + row) * image.width + x) * 4);
    }
    return { sheet, x, y };
  });
  return { ok: true, packed: { unique, indexOf, frames, sheets } };
}
