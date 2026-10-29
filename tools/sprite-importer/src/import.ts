// `import` (007 RF-B2, plan-b decision 13): everything in memory until the end. Destination
// first, then the export, the manifest, validateManifest, validateGuide against the ruleset
// golden, and only when all of it passes, the disk.
import { PNG } from 'pngjs';
import {
  GUIDE_LIMITS,
  validateGuide,
  type GuideError,
  type RgbaImage,
} from '@torneo/renderer/guide';
import {
  validateManifest,
  type AnimationCode,
  type SpriteManifest,
} from '@torneo/renderer/manifest';
import { CLI_CODES, EXPORT_CODES, IO_CODES } from './codes.ts';
import { EXPORT_FILE, readExportFrames, readExportHead, type ExportHead } from './export.ts';
import { loadGolden } from './golden.ts';
import { PIVOT } from './limits.ts';
import { pack, type Packed } from './pack.ts';
import { IssueSink, TooManyIssues, issue, type Issue } from './report.ts';
import { checkDestination, writeSet, type EncodedSheet } from './write.ts';

export interface ImportSummary {
  readonly frames: number;
  readonly sheets: number;
  readonly colors: number;
  readonly partial_alpha_pixels: number;
}

export interface ImportResult {
  /** 0 written, 1 the export does not meet the standard, 2 usage or I/O. */
  readonly status: 0 | 1 | 2;
  readonly issues: readonly Issue[];
  readonly summary?: ImportSummary;
}

export interface ImportOptions {
  /** The repository root (absolute). */
  readonly root: string;
  /** The export folder (absolute). */
  readonly inDir: string;
  /** The destination (absolute). */
  readonly outDir: string;
}

function fail(status: 1 | 2, issues: readonly Issue[]): ImportResult {
  return { status, issues };
}

/** The manifest of RF-B2: one frame per unique content, the pivot of the standard, no scale. */
export function buildManifest(head: ExportHead, packed: Packed): Record<string, unknown> {
  const cell = GUIDE_LIMITS.cellPx;
  const animations: Record<string, unknown> = {};
  let at = 0;
  for (const [code, anim] of head.animations) {
    const frames = packed.indexOf.slice(at, at + anim.frame_ms.length);
    at += anim.frame_ms.length;
    animations[code] = {
      frames,
      frame_ms: anim.frame_ms,
      loop: anim.loop,
      ...(anim.hasImpactFrame ? { impact_frame: anim.impact_frame } : {}),
    };
  }
  return {
    format: 'torneo.sprites',
    format_version: 1,
    id: head.id,
    pixel_art: head.pixel_art,
    sheets: packed.sheets.map((s, k) => ({
      file: `sheet-${k}.png`,
      width: s.width,
      height: s.height,
    })),
    frames: packed.frames.map((f) => ({
      sheet: f.sheet,
      x: f.x,
      y: f.y,
      w: cell,
      h: cell,
      pivot_x: PIVOT.x,
      pivot_y: PIVOT.y,
    })),
    animations,
  };
}

const ANIMATION_FRAME = /^\/animations\/([a-z_]+)\/frames\/([0-9]+)(?:\/|$)/;
const MANIFEST_FRAME = /^\/frames\/([0-9]+)(?:\/|$)/;

/**
 * The export file a guide error points at: the PNG of an animation step, the first PNG of a
 * manifest frame (its path stays the manifest pointer), or export.json.
 */
function guideFile(
  error: GuideError,
  head: ExportHead,
  packed: Packed,
  files: readonly string[],
): string {
  const step = ANIMATION_FRAME.exec(error.path);
  if (step !== null) {
    let at = 0;
    for (const [code, anim] of head.animations) {
      if (code === (step[1] as AnimationCode)) {
        return files[at + Number(step[2])] ?? EXPORT_FILE;
      }
      at += anim.frame_ms.length;
    }
  }
  const frame = MANIFEST_FRAME.exec(error.path);
  if (frame !== null) {
    const first = packed.indexOf.indexOf(Number(frame[1]));
    if (first >= 0) return files[first] ?? EXPORT_FILE;
  }
  return EXPORT_FILE;
}

/** Distinct RGBA colors among visible pixels, and pixels with alpha in 1..254 (RF-B7). */
export function paletteStats(cells: readonly Uint8Array[]): {
  colors: number;
  partial_alpha_pixels: number;
} {
  const colors = new Set<number>();
  let partial = 0;
  for (const cell of cells) {
    for (let i = 0; i < cell.length; i += 4) {
      const a = cell[i + 3] ?? 0;
      if (a === 0) continue;
      if (a < 255) partial++;
      colors.add(
        (((cell[i] ?? 0) << 24) | ((cell[i + 1] ?? 0) << 16) | ((cell[i + 2] ?? 0) << 8) | a) >>> 0,
      );
    }
  }
  return { colors: colors.size, partial_alpha_pixels: partial };
}

function encode(sheets: readonly RgbaImage[]): EncodedSheet[] {
  return sheets.map((s, k) => {
    const png = new PNG({ width: s.width, height: s.height });
    png.data.set(s.data);
    return { file: `sheet-${k}.png`, bytes: PNG.sync.write(png, { colorType: 6 }) };
  });
}

export function runImport(options: ImportOptions): ImportResult {
  const destination = checkDestination(options.root, options.outDir);
  if (!destination.ok) return fail(2, [destination.issue]);
  const rulesets = loadGolden(options.root);
  if (rulesets === null) return fail(2, [issue(IO_CODES.goldenInvalid, '', '')]);

  const sink = new IssueSink();
  let head: ExportHead | null;
  let packed: Packed;
  let manifest: SpriteManifest;
  let files: string[];
  try {
    head = readExportHead(sink, options.inDir);
    if (head === null) return fail(1, sink.issues);
    if (head.id !== destination.destination.id) {
      return fail(2, [issue(CLI_CODES.outIdMismatch, destination.destination.rel, '')]);
    }
    const frames = readExportFrames(sink, options.inDir, head);
    if (frames === null) return fail(1, sink.issues);
    files = frames.map((f) => f.file);
    const result = pack(frames.map((f) => f.rgba));
    if (!result.ok) {
      return fail(1, [
        issue(EXPORT_CODES.tooManyFrames, EXPORT_FILE, '', {
          frames: result.frames,
          max: result.max,
        }),
      ]);
    }
    packed = result.packed;
    const validated = validateManifest(buildManifest(head, packed), { expectedId: head.id });
    if (!validated.ok) {
      return fail(
        1,
        validated.errors.map((e) => issue(e.code, EXPORT_FILE, e.path, e.data)),
      );
    }
    manifest = validated.manifest;
  } catch (error) {
    if (error instanceof TooManyIssues) return fail(1, sink.issues);
    throw error;
  }

  const guide = validateGuide(manifest, packed.sheets, rulesets);
  if (!guide.ok) {
    return fail(
      1,
      guide.errors.map((e) => issue(e.code, guideFile(e, head, packed, files), e.path, e.data)),
    );
  }

  const sheets = encode(packed.sheets);
  try {
    writeSet(destination.destination.out, JSON.stringify(manifest, null, 2) + '\n', sheets);
  } catch {
    return fail(2, [issue(IO_CODES.writeFailed, destination.destination.rel, '')]);
  }
  return {
    status: 0,
    issues: [],
    summary: {
      frames: packed.unique.length,
      sheets: packed.sheets.length,
      ...paletteStats(packed.unique),
    },
  };
}
