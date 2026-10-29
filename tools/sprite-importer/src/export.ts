// Reading the open export, `torneo.sprites.export` v1 (007 RF-B1, plan-b decision 12). In order,
// and without decoding any PNG until everything before passes: the root, export.json, the frame
// folders, then each PNG (size, IHDR, decode). `file` is relative to the export folder and `path`
// uses the names of the manifest, so a manifest or guide error reads the same on the export.
// The values of `frame_ms`, `loop` and `impact_frame` pass as they are: validateManifest checks
// them (one validator).
import path from 'node:path';
import { PNG } from 'pngjs';
import { GUIDE_LIMITS } from '@torneo/renderer/guide';
import {
  LIMITS,
  OPTIONAL_ANIMATIONS,
  REQUIRED_ANIMATIONS,
  type AnimationCode,
} from '@torneo/renderer/manifest';
import { EXPORT_CODES as E } from './codes.ts';
import { kindAt, listDir, readBounded } from './fsx.ts';
import { readIhdr } from './ihdr.ts';
import { IssueSink, relFile } from './report.ts';

export const EXPORT_FORMAT = 'torneo.sprites.export';
export const EXPORT_FORMAT_VERSION = 1;
export const EXPORT_FILE = 'export.json';
export const FRAMES_DIR = 'frames';

/** Every animation code, required first, in the canonical order of the manifest. */
export const CANONICAL_ANIMATIONS: readonly AnimationCode[] = [
  ...REQUIRED_ANIMATIONS,
  ...OPTIONAL_ANIMATIONS,
];

const ROOT_KEYS = ['format', 'format_version', 'id', 'pixel_art', 'animations'] as const;
const ANIMATION_KEYS = ['frame_ms', 'loop', 'impact_frame'] as const;
const ANIMATION_REQUIRED = ['frame_ms', 'loop'] as const;
const FRAME_NAME = /^([0-9]{2})\.png$/;

/** One declared animation, its values untouched. */
export interface ExportAnimation {
  readonly frame_ms: readonly unknown[];
  readonly loop: unknown;
  readonly impact_frame?: unknown;
  readonly hasImpactFrame: boolean;
}

/** export.json and the frame folders, checked; no PNG opened yet. */
export interface ExportHead {
  readonly id: string;
  readonly pixel_art: boolean;
  /** Declared animations, in canonical order. */
  readonly animations: ReadonlyMap<AnimationCode, ExportAnimation>;
}

/** One frame of the export, decoded: 8-bit RGBA of `cellPx × cellPx`. */
export interface ExportFrame {
  readonly animation: AnimationCode;
  readonly step: number;
  /** Relative to the export folder: `frames/<animation>/<nn>.png`. */
  readonly file: string;
  readonly rgba: Uint8Array;
}

export interface Export extends ExportHead {
  /** Every frame, in canonical order (animation, then step). */
  readonly frames: readonly ExportFrame[];
}

const isObject = (value: unknown): value is Record<string, unknown> => {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false;
  const proto: unknown = Object.getPrototypeOf(value);
  return proto === Object.prototype || proto === null;
};

const own = (obj: Record<string, unknown>, key: string): unknown =>
  Object.hasOwn(obj, key) ? obj[key] : undefined;

const pointer = (...segments: readonly (string | number)[]): string =>
  segments.map((s) => '/' + String(s).replaceAll('~', '~0').replaceAll('/', '~1')).join('');

export const frameFile = (animation: string, step: number): string =>
  relFile(FRAMES_DIR, animation, `${String(step).padStart(2, '0')}.png`);

/** 1. The root holds exactly export.json (a file) and frames/ (a folder). */
function checkRoot(sink: IssueSink, dir: string): void {
  const seen = new Set<string>();
  for (const entry of listDir(dir)) {
    seen.add(entry.name);
    const expected =
      (entry.name === EXPORT_FILE && entry.kind === 'file') ||
      (entry.name === FRAMES_DIR && entry.kind === 'dir');
    if (entry.kind === 'symlink') sink.add(E.symlink, entry.name, '');
    else if (!expected) sink.add(E.unexpectedEntry, entry.name, '');
  }
  for (const name of [EXPORT_FILE, FRAMES_DIR]) {
    if (!seen.has(name)) sink.add(E.missingFile, name, '');
  }
}

function checkKeys(
  sink: IssueSink,
  value: Record<string, unknown>,
  allowed: readonly string[],
  required: readonly string[],
  base: readonly string[],
): void {
  for (const key of Object.keys(value)) {
    if (!allowed.includes(key)) sink.add(E.unknownField, EXPORT_FILE, pointer(...base, key));
  }
  for (const key of required) {
    if (!Object.hasOwn(value, key)) sink.add(E.missingField, EXPORT_FILE, pointer(...base, key));
  }
}

/** 2. export.json: bounded, JSON, exact keys, format, id, pixel_art and the animations. */
function readJson(sink: IssueSink, dir: string): ExportHead | null {
  const read = readBounded(path.join(dir, EXPORT_FILE), GUIDE_LIMITS.exportFileMaxBytes);
  if (!read.ok) {
    if (read.reason === 'too_large') sink.add(E.fileTooLarge, EXPORT_FILE, '');
    else if (read.reason === 'symlink' || read.reason === 'changed')
      sink.add(E.symlink, EXPORT_FILE, '');
    else sink.add(E.missingFile, EXPORT_FILE, '');
    return null;
  }
  let json: unknown;
  try {
    json = JSON.parse(read.bytes.toString('utf8'));
  } catch {
    sink.add(E.invalidJson, EXPORT_FILE, '');
    return null;
  }
  if (!isObject(json)) {
    sink.add(E.invalidValue, EXPORT_FILE, '');
    return null;
  }
  const before = sink.count;
  checkKeys(sink, json, ROOT_KEYS, ROOT_KEYS, []);
  const format = own(json, 'format');
  if (Object.hasOwn(json, 'format') && format !== EXPORT_FORMAT) {
    sink.add(E.unsupportedFormat, EXPORT_FILE, pointer('format'));
  }
  const version = own(json, 'format_version');
  if (Object.hasOwn(json, 'format_version') && version !== EXPORT_FORMAT_VERSION) {
    sink.add(E.unsupportedFormat, EXPORT_FILE, pointer('format_version'));
  }
  const id = own(json, 'id');
  if (Object.hasOwn(json, 'id') && (typeof id !== 'string' || !LIMITS.idPattern.test(id))) {
    sink.add(E.invalidValue, EXPORT_FILE, pointer('id'));
  }
  const pixelArt = own(json, 'pixel_art');
  if (Object.hasOwn(json, 'pixel_art') && typeof pixelArt !== 'boolean') {
    sink.add(E.invalidValue, EXPORT_FILE, pointer('pixel_art'));
  }
  const animations = new Map<AnimationCode, ExportAnimation>();
  const raw = own(json, 'animations');
  if (Object.hasOwn(json, 'animations') && !isObject(raw)) {
    sink.add(E.invalidValue, EXPORT_FILE, pointer('animations'));
  } else if (isObject(raw)) {
    const known: readonly string[] = CANONICAL_ANIMATIONS;
    for (const key of Object.keys(raw)) {
      if (!known.includes(key))
        sink.add(E.unknownAnimation, EXPORT_FILE, pointer('animations', key));
    }
    for (const code of CANONICAL_ANIMATIONS) {
      if (!Object.hasOwn(raw, code)) continue;
      const anim = raw[code];
      const base = ['animations', code];
      if (!isObject(anim)) {
        sink.add(E.invalidValue, EXPORT_FILE, pointer(...base));
        continue;
      }
      checkKeys(sink, anim, ANIMATION_KEYS, ANIMATION_REQUIRED, base);
      const frameMs = own(anim, 'frame_ms');
      if (
        Object.hasOwn(anim, 'frame_ms') &&
        (!Array.isArray(frameMs) ||
          frameMs.length < 1 ||
          frameMs.length > LIMITS.animationFramesMax)
      ) {
        sink.add(E.invalidValue, EXPORT_FILE, pointer(...base, 'frame_ms'));
      }
      if (Array.isArray(frameMs)) {
        const hasImpactFrame = Object.hasOwn(anim, 'impact_frame');
        animations.set(code, {
          frame_ms: frameMs,
          loop: own(anim, 'loop'),
          ...(hasImpactFrame ? { impact_frame: own(anim, 'impact_frame') } : {}),
          hasImpactFrame,
        });
      }
    }
  }
  if (sink.count > before) return null;
  return { id: id as string, pixel_art: pixelArt as boolean, animations };
}

/** 3. frames/ holds one folder per declared animation, each with 00.png … NN.png contiguous. */
function checkFolders(sink: IssueSink, dir: string, head: ExportHead): void {
  const framesDir = path.join(dir, FRAMES_DIR);
  const declared = new Set<string>(head.animations.keys());
  const present = new Set<string>();
  for (const entry of listDir(framesDir)) {
    const file = relFile(FRAMES_DIR, entry.name);
    // A declared name counts as present whatever its kind, so it is reported once.
    if (declared.has(entry.name)) present.add(entry.name);
    if (entry.kind === 'symlink') sink.add(E.symlink, file, '');
    else if (entry.kind !== 'dir' || !declared.has(entry.name))
      sink.add(E.unexpectedEntry, file, '');
  }
  for (const [code, anim] of head.animations) {
    const base = ['animations', code, 'frames'] as const;
    if (!present.has(code)) {
      sink.add(E.frameMissing, relFile(FRAMES_DIR, code), pointer(...base));
      continue;
    }
    if (kindAt(path.join(framesDir, code)) !== 'dir') continue;
    const steps = new Set<number>();
    for (const entry of listDir(path.join(framesDir, code))) {
      const file = relFile(FRAMES_DIR, code, entry.name);
      const match = FRAME_NAME.exec(entry.name);
      const named = match === null ? undefined : Number(match[1]);
      // A step with a valid name counts as present whatever its kind (reported once).
      if (named !== undefined && named < anim.frame_ms.length) steps.add(named);
      if (entry.kind === 'symlink') {
        sink.add(E.symlink, file, '');
      } else if (entry.kind !== 'file' || match === null) {
        sink.add(E.unexpectedEntry, file, '');
      } else {
        const step = Number(match[1]);
        if (step >= anim.frame_ms.length) sink.add(E.frameExtra, file, pointer(...base, step));
      }
    }
    for (let step = 0; step < anim.frame_ms.length; step++) {
      if (!steps.has(step)) sink.add(E.frameMissing, frameFile(code, step), pointer(...base, step));
    }
  }
}

type Png = { readonly ok: true; readonly bytes: Buffer } | { readonly ok: false };

/** Size and IHDR of one PNG; the bytes when they pass. */
function readPng(sink: IssueSink, dir: string, file: string, at: string): Png {
  const read = readBounded(path.join(dir, ...file.split('/')), GUIDE_LIMITS.exportFileMaxBytes);
  if (!read.ok) {
    if (read.reason === 'too_large') sink.add(E.fileTooLarge, file, at);
    else if (read.reason === 'missing') sink.add(E.frameMissing, file, at);
    else if (read.reason === 'not_file') sink.add(E.unexpectedEntry, file, at);
    else sink.add(E.symlink, file, at);
    return { ok: false };
  }
  const ihdr = readIhdr(read.bytes);
  if (ihdr === null) {
    sink.add(E.pngInvalid, file, at);
    return { ok: false };
  }
  const cell = GUIDE_LIMITS.cellPx;
  if (ihdr.width !== cell || ihdr.height !== cell) {
    sink.add(E.pngSize, file, at, { w: ihdr.width, h: ihdr.height });
    return { ok: false };
  }
  // pngjs inflates an interlaced PNG without a cap (a decompression bomb): never decoded.
  if (ihdr.interlace !== 0) {
    sink.add(E.pngInterlaced, file, at, { interlace: ihdr.interlace });
    return { ok: false };
  }
  return { ok: true, bytes: read.bytes };
}

/** Steps 1 to 3: everything but the PNG. `null` when something failed (the sink says what). */
export function readExportHead(sink: IssueSink, dir: string): ExportHead | null {
  const before = sink.count;
  checkRoot(sink, dir);
  if (sink.count > before) return null;
  const head = readJson(sink, dir);
  if (head === null) return null;
  checkFolders(sink, dir, head);
  return sink.count > before ? null : head;
}

/**
 * Step 4: the size and IHDR of every PNG first, and only when all pass, the decoder. pngjs always
 * gives 8-bit RGBA (palette, gray, RGB and 16 bits included); its length is checked anyway.
 */
export function readExportFrames(
  sink: IssueSink,
  dir: string,
  head: ExportHead,
): readonly ExportFrame[] | null {
  const before = sink.count;
  const list: { animation: AnimationCode; step: number; file: string; at: string }[] = [];
  for (const [animation, anim] of head.animations) {
    for (let step = 0; step < anim.frame_ms.length; step++) {
      list.push({
        animation,
        step,
        file: frameFile(animation, step),
        at: pointer('animations', animation, 'frames', step),
      });
    }
  }
  for (const f of list) readPng(sink, dir, f.file, f.at);
  if (sink.count > before) return null;
  const cell = GUIDE_LIMITS.cellPx;
  const frames: ExportFrame[] = [];
  for (const f of list) {
    // Read again (one file in memory at a time) and checked again before decoding.
    const png = readPng(sink, dir, f.file, f.at);
    if (!png.ok) continue;
    let rgba: Uint8Array;
    try {
      const decoded = PNG.sync.read(png.bytes);
      rgba = new Uint8Array(decoded.data);
      if (decoded.width !== cell || decoded.height !== cell || rgba.length !== cell * cell * 4) {
        throw new RangeError('size');
      }
    } catch {
      sink.add(E.pngInvalid, f.file, f.at);
      continue;
    }
    frames.push({ animation: f.animation, step: f.step, file: f.file, rgba });
  }
  return sink.count > before ? null : frames;
}

/** The whole export (RF-B1), or `null` with the issues in the sink. */
export function readExport(sink: IssueSink, dir: string): Export | null {
  const head = readExportHead(sink, dir);
  if (head === null) return null;
  const frames = readExportFrames(sink, dir, head);
  return frames === null ? null : { ...head, frames };
}
