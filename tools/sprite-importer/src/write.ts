// The destination of `import` and the writing (007 RF-B2, plan-b decisions 13 and 19). `--out`
// is exactly `<root>/content/sprites/<id>` or `<root>/content/_local/sprites/<id>`; no component
// from the root down is a symlink; an existing `<id>` only holds what the importer knows. Nothing
// here reads the export, so a refusal costs nothing.
import { mkdirSync, unlinkSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { LIMITS } from '@torneo/renderer/manifest';
import { CLI_CODES as C } from './codes.ts';
import { kindAt, listDir } from './fsx.ts';
import { relFile, type Issue, issue } from './report.ts';

export const MANIFEST_FILE = 'manifest.json';
export const PROVENANCE_FILE = 'provenance.json';

export interface Destination {
  readonly out: string;
  /** The folder name, which must be the export id. */
  readonly id: string;
  /** `content/sprites/<id>` (committed) or `content/_local/sprites/<id>`. */
  readonly committed: boolean;
  /** Relative to the root, for the output lines. */
  readonly rel: string;
}

export type DestinationResult =
  | { readonly ok: true; readonly destination: Destination }
  | { readonly ok: false; readonly issue: Issue };

const COMMITTED = ['content', 'sprites'] as const;
const LOCAL = ['content', '_local', 'sprites'] as const;

function startsWith(parts: readonly string[], prefix: readonly string[]): boolean {
  return parts.length === prefix.length + 1 && prefix.every((p, i) => parts[i] === p);
}

/** Checks `out` (absolute, already resolved) against `root` (absolute). */
export function checkDestination(root: string, out: string): DestinationResult {
  const relative = path.relative(root, out);
  const parts = relative === '' ? [] : relative.split(path.sep);
  const committed = startsWith(parts, COMMITTED);
  const id = parts.at(-1) ?? '';
  if (
    path.isAbsolute(relative) ||
    (!committed && !startsWith(parts, LOCAL)) ||
    !LIMITS.idPattern.test(id)
  ) {
    return { ok: false, issue: issue(C.outForbidden, '', '') };
  }
  const rel = relFile(...parts);
  for (let i = 1; i <= parts.length; i++) {
    const kind = kindAt(path.join(root, ...parts.slice(0, i)));
    const at = relFile(...parts.slice(0, i));
    if (kind === undefined) break;
    if (kind === 'symlink') return { ok: false, issue: issue(C.outSymlink, at, '') };
    if (kind !== 'dir') return { ok: false, issue: issue(C.outNotADirectory, at, '') };
  }
  if (kindAt(out) === 'dir') {
    for (const entry of listDir(out)) {
      const known =
        entry.kind === 'file' &&
        (entry.name === MANIFEST_FILE ||
          LIMITS.sheetFilePattern.test(entry.name) ||
          (committed && entry.name === PROVENANCE_FILE));
      if (!known) {
        return { ok: false, issue: issue(C.outUnexpectedEntry, relFile(rel, entry.name), '') };
      }
    }
  }
  return { ok: true, destination: { out, id, committed, rel } };
}

export interface EncodedSheet {
  readonly file: string;
  readonly bytes: Uint8Array;
}

/**
 * Creates the folder if missing, deletes the previous sheets, writes the new ones and the
 * manifest last. provenance.json is never touched. Throws on an I/O error (`io.write_failed`
 * for the caller); a half-written folder never passes `check`.
 */
export function writeSet(out: string, manifestJson: string, sheets: readonly EncodedSheet[]): void {
  mkdirSync(out, { recursive: true });
  for (const entry of listDir(out)) {
    if (entry.kind === 'file' && LIMITS.sheetFilePattern.test(entry.name)) {
      unlinkSync(path.join(out, entry.name));
    }
  }
  for (const sheet of sheets) writeFileSync(path.join(out, sheet.file), sheet.bytes);
  writeFileSync(path.join(out, MANIFEST_FILE), manifestJson);
}
