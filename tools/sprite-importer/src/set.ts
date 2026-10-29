// One committed set, `content/sprites/<id>/` (007 RF-B3, RF-B4, plan-b decision 14): exactly
// manifest.json, provenance.json and the sheets the manifest declares; validateManifest with the
// folder name; each sheet bounded and with the IHDR of `sheets[]` in 8-bit RGBA; only then the
// decoder and validateGuide against every ruleset version. A sheet that fails its header or the
// decoder never reaches validateGuide, so a set of the repo never makes it throw.
import path from 'node:path';
import { PNG } from 'pngjs';
import {
  GUIDE_LIMITS,
  validateGuide,
  type RgbaImage,
  type RulesetTiming,
} from '@torneo/renderer/guide';
import { LIMITS, validateManifest, type SpriteManifest } from '@torneo/renderer/manifest';
import { PROVENANCE_CODES, SET_CODES as S } from './codes.ts';
import { listDir, readBounded, type EntryKind, type ReadResult } from './fsx.ts';
import { readIhdr } from './ihdr.ts';
import { TOOL_LIMITS } from './limits.ts';
import { checkProvenance } from './provenance.ts';
import { relFile, type IssueSink } from './report.ts';
import { MANIFEST_FILE, PROVENANCE_FILE } from './write.ts';

export const SETS_DIR = ['content', 'sprites'] as const;

const RGBA_COLOR_TYPE = 6;
const RGBA_BIT_DEPTH = 8;

function parseJson(bytes: Buffer): { ok: true; json: unknown } | { ok: false } {
  try {
    return { ok: true, json: JSON.parse(bytes.toString('utf8')) };
  } catch {
    return { ok: false };
  }
}

type ReadFailure = Extract<ReadResult, { ok: false }>['reason'];

/**
 * Reports a failed read, so it never passes in silence (QA nit, PR #52). An entry the listing
 * did not see as a regular file is already `set.unexpected_entry`; anything else that fails here
 * changed between the listing and the read.
 */
export function reportReadFailure(
  sink: IssueSink,
  file: string,
  at: string,
  reason: ReadFailure,
  listed: EntryKind | undefined,
  tooLarge: string,
): void {
  if (reason === 'missing') sink.add(S.missingFile, file, at);
  else if (reason === 'too_large') sink.add(tooLarge, file, at);
  else if (listed === 'file') sink.add(S.unexpectedEntry, file, at);
}

function readManifest(
  sink: IssueSink,
  dir: string,
  rel: string,
  id: string,
  listed: EntryKind | undefined,
): SpriteManifest | null {
  const file = relFile(rel, MANIFEST_FILE);
  const read = readBounded(path.join(dir, MANIFEST_FILE), TOOL_LIMITS.jsonFileMaxBytes);
  if (!read.ok) {
    reportReadFailure(sink, file, '', read.reason, listed, S.fileTooLarge);
    return null;
  }
  const parsed = parseJson(read.bytes);
  if (!parsed.ok) {
    sink.add(S.invalidJson, file, '');
    return null;
  }
  const result = validateManifest(parsed.json, { expectedId: id });
  if (!result.ok) {
    for (const e of result.errors) sink.add(e.code, file, e.path, e.data);
    return null;
  }
  return result.manifest;
}

/** Bounded, IHDR equal to `sheets[i]` in 8-bit RGBA, then decoded; `null` when one fails. */
function readSheets(
  sink: IssueSink,
  dir: string,
  rel: string,
  manifest: SpriteManifest,
): RgbaImage[] | null {
  const before = sink.count;
  const bytes: Buffer[] = [];
  manifest.sheets.forEach((sheet, i) => {
    const file = relFile(rel, sheet.file);
    const at = `/sheets/${i}`;
    const read = readBounded(path.join(dir, sheet.file), GUIDE_LIMITS.sheetFileMaxBytes);
    if (!read.ok) {
      // Every declared sheet was listed as a regular file before this is called.
      reportReadFailure(sink, file, at, read.reason, 'file', S.fileTooLarge);
      return;
    }
    const ihdr = readIhdr(read.bytes);
    if (ihdr === null) {
      sink.add(S.sheetInvalid, file, at);
      return;
    }
    if (
      ihdr.width !== sheet.width ||
      ihdr.height !== sheet.height ||
      ihdr.bitDepth !== RGBA_BIT_DEPTH ||
      ihdr.colorType !== RGBA_COLOR_TYPE ||
      // pngjs inflates an interlaced PNG without a cap (a decompression bomb): never decoded.
      ihdr.interlace !== 0
    ) {
      sink.add(S.sheetHeader, file, at, {
        width: ihdr.width,
        height: ihdr.height,
        bit_depth: ihdr.bitDepth,
        color_type: ihdr.colorType,
        interlace: ihdr.interlace,
      });
      return;
    }
    bytes.push(read.bytes);
  });
  if (sink.count > before || bytes.length !== manifest.sheets.length) return null;
  const images: RgbaImage[] = [];
  manifest.sheets.forEach((sheet, i) => {
    try {
      const png = PNG.sync.read(bytes[i] as Buffer);
      const data = new Uint8Array(png.data);
      if (png.width !== sheet.width || png.height !== sheet.height) throw new RangeError('size');
      if (data.length !== sheet.width * sheet.height * 4) throw new RangeError('size');
      images.push({ width: png.width, height: png.height, data });
    } catch {
      sink.add(S.sheetInvalid, relFile(rel, sheet.file), `/sheets/${i}`);
    }
  });
  return sink.count > before ? null : images;
}

function readProvenance(
  sink: IssueSink,
  dir: string,
  rel: string,
  id: string,
  listed: EntryKind | undefined,
): void {
  const file = relFile(rel, PROVENANCE_FILE);
  const read = readBounded(path.join(dir, PROVENANCE_FILE), TOOL_LIMITS.jsonFileMaxBytes);
  if (!read.ok) {
    reportReadFailure(sink, file, '', read.reason, listed, PROVENANCE_CODES.fileTooLarge);
    return;
  }
  const parsed = parseJson(read.bytes);
  if (!parsed.ok) {
    sink.add(PROVENANCE_CODES.invalidJson, file, '');
    return;
  }
  checkProvenance(sink, file, parsed.json, id);
}

export function checkSet(
  sink: IssueSink,
  root: string,
  id: string,
  rulesets: readonly RulesetTiming[],
): void {
  const rel = relFile(...SETS_DIR, id);
  const dir = path.join(root, ...SETS_DIR, id);
  const entries = new Map<string, EntryKind>(listDir(dir).map((e) => [e.name, e.kind]));
  const manifest = readManifest(sink, dir, rel, id, entries.get(MANIFEST_FILE));
  const declared = manifest === null ? null : new Set(manifest.sheets.map((s) => s.file));
  for (const [name, kind] of entries) {
    const known =
      name === MANIFEST_FILE ||
      name === PROVENANCE_FILE ||
      (declared === null ? LIMITS.sheetFilePattern.test(name) : declared.has(name));
    if (kind !== 'file' || !known) sink.add(S.unexpectedEntry, relFile(rel, name), '');
  }
  if (manifest !== null && declared !== null) {
    const missing = [...declared].filter((name) => !entries.has(name));
    for (const name of missing) sink.add(S.missingFile, relFile(rel, name), '');
    const allFiles = [...declared].every((name) => entries.get(name) === 'file');
    if (allFiles) {
      // At most sheetsMax sheets of 2048² (64 MB), released before the next set.
      const sheets = readSheets(sink, dir, rel, manifest);
      if (sheets !== null) {
        const guide = validateGuide(manifest, sheets, rulesets);
        if (!guide.ok) {
          for (const e of guide.errors)
            sink.add(e.code, relFile(rel, MANIFEST_FILE), e.path, e.data);
        }
      }
    }
  }
  readProvenance(sink, dir, rel, id, entries.get(PROVENANCE_FILE));
}
