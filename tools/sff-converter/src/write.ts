// Writing the output (002 RF-B5, plan-b decision 12). Every check runs before touching the disk,
// so a refusal never leaves a directory nor a file outside what git ignores. No try/catch here
// (plan-b decision 3): spawnSync does not throw, and nearestExisting never lstats through a file.
import { spawnSync } from 'node:child_process';
import {
  existsSync,
  lstatSync,
  mkdirSync,
  readdirSync,
  statSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import { PNG } from 'pngjs';
import { validateManifest, type SpriteManifest } from '@torneo/renderer/manifest';
import { fail } from './errors.ts';
import type { SheetImage } from './convert.ts';

/** The pre-push hook runs with these set; they would point git at the real repository. */
const GIT_VARS = [
  'GIT_DIR',
  'GIT_WORK_TREE',
  'GIT_INDEX_FILE',
  'GIT_COMMON_DIR',
  'GIT_OBJECT_DIRECTORY',
];
const OLD_OUTPUT = /^(?:manifest\.json|sheet-[0-9]+\.png)$/;
/**
 * Empty file that marks a folder as written by this converter (002 plan-c decision 9 b). Old
 * output is only deleted next to it, so `--out` never wipes files it did not write.
 */
export const OUTPUT_MARKER = '.sff-converter-output';

export interface WriteOptions {
  /** Base environment of the `git` child (default: process.env); the GIT_* above are removed. */
  readonly env?: NodeJS.ProcessEnv;
}

export function cleanGitEnv(env: NodeJS.ProcessEnv): NodeJS.ProcessEnv {
  const out: NodeJS.ProcessEnv = { ...env };
  for (const name of GIT_VARS) delete out[name];
  return out;
}

/**
 * The nearest existing ancestor of `dir` (or `dir` itself), walking down from the root so that
 * no `lstat` ever goes through a file (that would throw ENOTDIR). It must be a real directory: a
 * symbolic link there is `write.symlink`; a file anywhere on the way is `write.not_a_directory`.
 * Links higher up are followed (macOS has `/var` → `/private/var`), checked with `existsSync`
 * and `statSync`, which only run once `existsSync` said the target resolves.
 */
function nearestExisting(dir: string): string {
  const parts = dir.split(path.sep).filter((part) => part !== '');
  let current = path.parse(dir).root;
  for (const part of parts) {
    const next = path.join(current, part);
    const stat = lstatSync(next, { throwIfNoEntry: false });
    if (stat === undefined) break;
    if (stat.isSymbolicLink()) {
      if (!existsSync(next)) fail('write.symlink');
      if (!statSync(next).isDirectory()) fail('write.not_a_directory');
    } else if (!stat.isDirectory()) {
      fail('write.not_a_directory');
    }
    current = next;
  }
  if (lstatSync(current, { throwIfNoEntry: false })?.isSymbolicLink() === true)
    fail('write.symlink');
  return current;
}

function checkIgnored(file: string, cwd: string, env: NodeJS.ProcessEnv): void {
  const result = spawnSync('git', ['check-ignore', '-q', '--', file], {
    cwd,
    env,
    shell: false,
    stdio: 'ignore',
  });
  if (result.error === undefined && result.signal === null && result.status === 0) return;
  if (result.error === undefined && result.signal === null && result.status === 1) {
    fail('write.not_ignored', { file: path.basename(file) });
  }
  fail('write.git_unavailable', { status: result.status ?? -1 });
}

export function writeOutput(
  outDir: string,
  manifest: SpriteManifest,
  sheets: readonly SheetImage[],
  options: WriteOptions = {},
): void {
  // 1. The manifest is valid and its sheets are the ones given.
  const result = validateManifest(manifest, { expectedId: manifest.id });
  if (!result.ok) fail('write.invalid_manifest', { errors: result.errors.length });
  if (
    sheets.length !== manifest.sheets.length ||
    sheets.some((s, i) => s.file !== manifest.sheets[i]?.file)
  ) {
    fail('write.sheet_mismatch');
  }
  const out = path.resolve(outDir);

  // 2. The nearest existing ancestor (or the output itself): a real directory.
  const existing = nearestExisting(out);

  // 3. Every file it writes is ignored by git (a `!` rule could bring back one of them).
  const env = cleanGitEnv(options.env ?? process.env);
  for (const file of ['manifest.json', ...sheets.map((s) => s.file), OUTPUT_MARKER]) {
    checkIgnored(path.join(out, file), existing, env);
  }

  // 4. Old output is only ours to delete next to the marker (a regular file, not a link).
  const marker = path.join(out, OUTPUT_MARKER);
  if (existing === out) {
    const markerStat = lstatSync(marker, { throwIfNoEntry: false });
    if (markerStat?.isSymbolicLink() === true) fail('write.symlink');
    const marked = markerStat?.isFile() === true;
    const old = readdirSync(out).filter((name) => OLD_OUTPUT.test(name));
    if (!marked && old.length > 0) fail('write.foreign_output', { file: old[0] ?? '' });
  }

  // 5. Only now, the disk: the marker first, so a run cut halfway can still be cleaned up.
  mkdirSync(out, { recursive: true });
  writeFileSync(marker, '');
  for (const name of readdirSync(out)) {
    if (OLD_OUTPUT.test(name)) unlinkSync(path.join(out, name));
  }
  for (const sheet of sheets) {
    const png = new PNG({ width: sheet.width, height: sheet.height });
    png.data.set(sheet.rgba);
    writeFileSync(path.join(out, sheet.file), PNG.sync.write(png, { colorType: 6 }));
  }
  writeFileSync(path.join(out, 'manifest.json'), JSON.stringify(result.manifest, null, 2) + '\n');
}
