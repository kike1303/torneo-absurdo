// Raw frames + recipe → the open export (004). The bridge between whatever produced the art and
// `import`: it knows no authoring tool, only PNG files and the numbers in the recipe. It scales by
// an integer with no filtering, cuts the alpha to binary, puts the feet on the pivot of the
// standard and, optionally, shares one small palette across the set. It does not validate the
// guide: `import` does, so there is a single judge of the standard.
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { GUIDE_LIMITS } from '@torneo/renderer/guide';
import { LIMITS } from '@torneo/renderer/manifest';
import { PNG } from 'pngjs';
import { NORMALIZE_CODES as C } from './codes.ts';
import { PIVOT } from './limits.ts';

export interface RecipeAnimation {
  /** Folder of `<in>/` with the raw frames, `<nn>.png`. Defaults to the name of the animation. */
  readonly source?: string;
  /** Indices of the raw frames, in playback order; a frame may repeat. */
  readonly frames: readonly number[];
  readonly frame_ms: readonly number[];
  readonly loop: boolean;
  readonly impact_frame?: number;
  /** `first`: one vertical shift for the whole animation; `each`: every frame on the ground. */
  readonly ground?: 'first' | 'each';
  /** Hand correction of the anchor, in pixels of the cell. */
  readonly offset?: readonly [number, number];
}

export interface Recipe {
  readonly format: 'torneo.sprites.recipe';
  readonly format_version: 1;
  readonly id: string;
  readonly scale: number;
  readonly max_colors?: number;
  readonly animations: Readonly<Record<string, RecipeAnimation>>;
}

export interface RawImage {
  readonly width: number;
  readonly height: number;
  readonly data: Uint8Array;
}

export interface Issue {
  readonly code: string;
  readonly path: string;
}

/** Rows of the bottom of the body used to find the center of the feet. */
const FEET_ROWS = 4;
/** Alpha at or above this is opaque; the rest is transparent. */
const ALPHA_CUT = 128;

const CELL = GUIDE_LIMITS.cellPx;
/** Nothing larger than a few pixels of art fits in the cell beyond this. */
const MAX_SCALE = 16;

const RECIPE_KEYS = new Set([
  'format',
  'format_version',
  'id',
  'scale',
  'max_colors',
  'animations',
]);
const ANIMATION_KEYS = new Set([
  'source',
  'frames',
  'frame_ms',
  'loop',
  'impact_frame',
  'ground',
  'offset',
]);

function isId(value: unknown): boolean {
  return typeof value === 'string' && LIMITS.idPattern.test(value);
}

function isOffset(value: unknown): boolean {
  return Array.isArray(value) && value.length === 2 && value.every((n) => Number.isInteger(n));
}

/**
 * The issues of a parsed recipe. Exact keys and a type for each: it holds numbers and modes, never
 * free text, and its names become paths, so none of them can leave its folder.
 */
export function validateRecipe(value: unknown): Issue[] {
  const issues: Issue[] = [];
  const r = value as Partial<Recipe> | null;
  if (typeof r !== 'object' || r === null) return [{ code: C.recipeInvalid, path: '' }];
  for (const key of Object.keys(r)) {
    if (!RECIPE_KEYS.has(key)) issues.push({ code: C.recipeInvalid, path: `/${key}` });
  }
  if (r.format !== 'torneo.sprites.recipe' || r.format_version !== 1) {
    issues.push({ code: C.recipeInvalid, path: '/format' });
  }
  if (!isId(r.id)) {
    issues.push({ code: C.recipeInvalid, path: '/id' });
  }
  if (!Number.isInteger(r.scale) || (r.scale ?? 0) < 1 || (r.scale ?? 0) > MAX_SCALE) {
    issues.push({ code: C.recipeInvalid, path: '/scale' });
  }
  if (r.max_colors !== undefined && (!Number.isInteger(r.max_colors) || r.max_colors < 2)) {
    issues.push({ code: C.recipeInvalid, path: '/max_colors' });
  }
  if (
    typeof r.animations !== 'object' ||
    r.animations === null ||
    Array.isArray(r.animations) ||
    Object.keys(r.animations).length === 0
  ) {
    issues.push({ code: C.recipeInvalid, path: '/animations' });
    return issues;
  }
  for (const [name, a] of Object.entries(r.animations)) {
    const at = `/animations/${name}`;
    if (typeof a !== 'object' || a === null) {
      issues.push({ code: C.recipeInvalid, path: at });
      continue;
    }
    if (!isId(name)) issues.push({ code: C.recipeInvalid, path: at });
    if (a.source !== undefined && !isId(a.source)) {
      issues.push({ code: C.recipeInvalid, path: `${at}/source` });
    }
    if (a.ground !== undefined && a.ground !== 'first' && a.ground !== 'each') {
      issues.push({ code: C.recipeInvalid, path: `${at}/ground` });
    }
    if (typeof a.loop !== 'boolean') issues.push({ code: C.recipeInvalid, path: `${at}/loop` });
    if (a.impact_frame !== undefined && (!Number.isInteger(a.impact_frame) || a.impact_frame < 0)) {
      issues.push({ code: C.recipeInvalid, path: `${at}/impact_frame` });
    }
    if (a.offset !== undefined && !isOffset(a.offset)) {
      issues.push({ code: C.recipeInvalid, path: `${at}/offset` });
    }
    if (Array.isArray(a.frame_ms) && !a.frame_ms.every((ms) => Number.isInteger(ms) && ms > 0)) {
      issues.push({ code: C.recipeInvalid, path: `${at}/frame_ms` });
    }
    for (const key of Object.keys(a)) {
      if (!ANIMATION_KEYS.has(key)) issues.push({ code: C.recipeInvalid, path: `${at}/${key}` });
    }
    if (
      !Array.isArray(a.frames) ||
      !Array.isArray(a.frame_ms) ||
      a.frames.length === 0 ||
      a.frames.length !== a.frame_ms.length ||
      !a.frames.every((f) => Number.isInteger(f) && f >= 0)
    ) {
      issues.push({ code: C.recipeInvalid, path: at });
    }
  }
  return issues;
}

/** Nearest-neighbor by an integer: every pixel becomes a `scale`×`scale` block. */
export function scaleImage(image: RawImage, scale: number): RawImage {
  const width = image.width * scale;
  const height = image.height * scale;
  const data = new Uint8Array(width * height * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const from = (Math.floor(y / scale) * image.width + Math.floor(x / scale)) * 4;
      data.set(image.data.subarray(from, from + 4), (y * width + x) * 4);
    }
  }
  return { width, height, data };
}

function isOpaque(image: RawImage, x: number, y: number): boolean {
  return (image.data[(y * image.width + x) * 4 + 3] ?? 0) >= ALPHA_CUT;
}

/** Lowest opaque row, or `undefined` for an empty frame. */
export function bottomRow(image: RawImage): number | undefined {
  for (let y = image.height - 1; y >= 0; y--) {
    for (let x = 0; x < image.width; x++) if (isOpaque(image, x, y)) return y;
  }
  return undefined;
}

/** Center of the opaque columns in the lowest `rows` rows of the body. */
export function feetCenter(image: RawImage, rows: number): number | undefined {
  const bottom = bottomRow(image);
  if (bottom === undefined) return undefined;
  let min = Infinity;
  let max = -Infinity;
  for (let y = Math.max(0, bottom - rows + 1); y <= bottom; y++) {
    for (let x = 0; x < image.width; x++) {
      if (isOpaque(image, x, y)) {
        min = Math.min(min, x);
        max = Math.max(max, x);
      }
    }
  }
  return Math.floor((min + max) / 2);
}

/** Copies `image` into a transparent cell, with `dx`, `dy`; what falls outside is cut. */
export function placeInCell(image: RawImage, dx: number, dy: number): RawImage {
  const data = new Uint8Array(CELL * CELL * 4);
  for (let y = 0; y < image.height; y++) {
    for (let x = 0; x < image.width; x++) {
      const tx = x + dx;
      const ty = y + dy;
      if (tx < 0 || ty < 0 || tx >= CELL || ty >= CELL || !isOpaque(image, x, y)) continue;
      const from = (y * image.width + x) * 4;
      const to = (ty * CELL + tx) * 4;
      data[to] = image.data[from] ?? 0;
      data[to + 1] = image.data[from + 1] ?? 0;
      data[to + 2] = image.data[from + 2] ?? 0;
      data[to + 3] = 255;
    }
  }
  return { width: CELL, height: CELL, data };
}

type Rgb = readonly [number, number, number];

function channelRange(colors: readonly Rgb[], c: 0 | 1 | 2): number {
  let min = 255;
  let max = 0;
  for (const color of colors) {
    min = Math.min(min, color[c]);
    max = Math.max(max, color[c]);
  }
  return max - min;
}

/**
 * Median cut over the distinct colors, weighted by nothing: deterministic, with no dependency, and
 * rare colors (an eye, a tusk) keep a say in the palette.
 */
export function medianCut(colors: readonly Rgb[], max: number): Rgb[] {
  const buckets: Rgb[][] = [[...colors]];
  while (buckets.length < max) {
    let best = -1;
    let bestRange = 0;
    buckets.forEach((b, i) => {
      if (b.length < 2) return;
      const range = Math.max(channelRange(b, 0), channelRange(b, 1), channelRange(b, 2));
      if (range > bestRange) {
        bestRange = range;
        best = i;
      }
    });
    if (best < 0) break;
    const bucket = buckets[best] as Rgb[];
    const ranges = [channelRange(bucket, 0), channelRange(bucket, 1), channelRange(bucket, 2)];
    const c = ranges.indexOf(Math.max(...ranges)) as 0 | 1 | 2;
    const sorted = [...bucket].sort(
      (a, b) => a[c] - b[c] || a[0] - b[0] || a[1] - b[1] || a[2] - b[2],
    );
    const mid = Math.floor(sorted.length / 2);
    buckets.splice(best, 1, sorted.slice(0, mid), sorted.slice(mid));
  }
  return buckets.map((b) => {
    const sum = b.reduce<[number, number, number]>(
      (acc, color) => [acc[0] + color[0], acc[1] + color[1], acc[2] + color[2]],
      [0, 0, 0],
    );
    return [
      Math.round(sum[0] / b.length),
      Math.round(sum[1] / b.length),
      Math.round(sum[2] / b.length),
    ];
  });
}

/** Rewrites the opaque pixels of every image with their nearest color of a shared palette. */
export function sharePalette(images: readonly RawImage[], max: number): number {
  const distinct = new Map<number, Rgb>();
  for (const image of images) {
    for (let i = 0; i < image.data.length; i += 4) {
      if ((image.data[i + 3] ?? 0) === 0) continue;
      const rgb: Rgb = [image.data[i] ?? 0, image.data[i + 1] ?? 0, image.data[i + 2] ?? 0];
      distinct.set((rgb[0] << 16) | (rgb[1] << 8) | rgb[2], rgb);
    }
  }
  const keys = [...distinct.keys()].sort((a, b) => a - b);
  const palette = medianCut(
    keys.map((k) => distinct.get(k) as Rgb),
    max,
  );
  const nearest = new Map<number, Rgb>();
  for (const key of keys) {
    const rgb = distinct.get(key) as Rgb;
    let best = palette[0] as Rgb;
    let bestDistance = Infinity;
    for (const color of palette) {
      const d = (color[0] - rgb[0]) ** 2 + (color[1] - rgb[1]) ** 2 + (color[2] - rgb[2]) ** 2;
      if (d < bestDistance) {
        bestDistance = d;
        best = color;
      }
    }
    nearest.set(key, best);
  }
  for (const image of images) {
    for (let i = 0; i < image.data.length; i += 4) {
      if ((image.data[i + 3] ?? 0) === 0) continue;
      const key =
        ((image.data[i] ?? 0) << 16) | ((image.data[i + 1] ?? 0) << 8) | (image.data[i + 2] ?? 0);
      const color = nearest.get(key) as Rgb;
      image.data[i] = color[0];
      image.data[i + 1] = color[1];
      image.data[i + 2] = color[2];
    }
  }
  return palette.length;
}

function pad(n: number): string {
  return String(n).padStart(2, '0');
}

/** Puts the frames of one animation in cells: the anchor comes from its first frame. */
export function normalizeFrames(
  raw: readonly RawImage[],
  anim: Pick<RecipeAnimation, 'ground' | 'offset'>,
  scale: number,
): RawImage[] {
  const scaled = raw.map((image) => scaleImage(image, scale));
  const first = scaled[0] as RawImage;
  const center = feetCenter(first, FEET_ROWS * scale) ?? Math.floor(first.width / 2);
  const firstBottom = bottomRow(first) ?? first.height - 1;
  const [ox, oy] = anim.offset ?? [0, 0];
  return scaled.map((image) => {
    const bottom = anim.ground === 'each' ? (bottomRow(image) ?? firstBottom) : firstBottom;
    return placeInCell(image, PIVOT.x - center + ox, PIVOT.y - 1 - bottom + oy);
  });
}

export interface NormalizeSummary {
  readonly frames: number;
  readonly colors_in: number;
  readonly palette: number | null;
}

export type NormalizeResult =
  | { readonly status: 0; readonly summary: NormalizeSummary }
  | { readonly status: 1; readonly issues: readonly Issue[] };

function readFrame(file: string): RawImage | undefined {
  try {
    const png = PNG.sync.read(readFileSync(file));
    return { width: png.width, height: png.height, data: new Uint8Array(png.data) };
  } catch {
    return undefined;
  }
}

function countColors(images: readonly RawImage[]): number {
  const seen = new Set<number>();
  for (const image of images) {
    for (let i = 0; i < image.data.length; i += 4) {
      if ((image.data[i + 3] ?? 0) === 0) continue;
      seen.add(
        ((image.data[i] ?? 0) << 16) | ((image.data[i + 1] ?? 0) << 8) | (image.data[i + 2] ?? 0),
      );
    }
  }
  return seen.size;
}

/** Writes `<out>/export.json` and `<out>/frames/`, replacing whatever `<out>` had. */
export function runNormalize(options: {
  readonly inDir: string;
  readonly recipe: Recipe;
  readonly outDir: string;
}): NormalizeResult {
  const { inDir, recipe, outDir } = options;
  const issues = validateRecipe(recipe);
  if (issues.length > 0) return { status: 1, issues };
  const cells = new Map<string, RawImage[]>();
  for (const [name, anim] of Object.entries(recipe.animations)) {
    const raw: RawImage[] = [];
    for (const index of anim.frames) {
      const rel = `${anim.source ?? name}/${pad(index)}.png`;
      const image = readFrame(path.join(inDir, ...rel.split('/')));
      if (image === undefined) issues.push({ code: C.sourceMissing, path: rel });
      else raw.push(image);
    }
    if (raw.length === anim.frames.length) {
      cells.set(name, normalizeFrames(raw, anim, recipe.scale));
    }
  }
  if (issues.length > 0) return { status: 1, issues };
  const all = [...cells.values()].flat();
  const colorsIn = countColors(all);
  const palette = recipe.max_colors === undefined ? null : sharePalette(all, recipe.max_colors);

  rmSync(outDir, { recursive: true, force: true });
  const animations: Record<string, unknown> = {};
  for (const [name, frames] of cells) {
    const anim = recipe.animations[name] as RecipeAnimation;
    animations[name] = {
      frame_ms: anim.frame_ms,
      loop: anim.loop,
      ...(anim.impact_frame === undefined ? {} : { impact_frame: anim.impact_frame }),
    };
    const dir = path.join(outDir, 'frames', name);
    mkdirSync(dir, { recursive: true });
    frames.forEach((image, i) => {
      const png = new PNG({ width: image.width, height: image.height });
      png.data.set(image.data);
      writeFileSync(path.join(dir, `${pad(i)}.png`), PNG.sync.write(png, { colorType: 6 }));
    });
  }
  const exportJson = {
    format: 'torneo.sprites.export',
    format_version: 1,
    id: recipe.id,
    pixel_art: true,
    animations,
  };
  writeFileSync(path.join(outDir, 'export.json'), JSON.stringify(exportJson, null, 2) + '\n');
  return { status: 0, summary: { frames: all.length, colors_in: colorsIn, palette } };
}
