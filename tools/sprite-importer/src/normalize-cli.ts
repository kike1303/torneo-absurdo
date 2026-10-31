// CLI of `normalize` (004), apart from cli.ts so that `import` and `check` never load it. Same
// contract: one JSON line per issue, exit 0 when it passes, 1 when the recipe or the sources are
// wrong, 2 on a usage or I/O error. It replaces its destination, so it only writes to
// `content/_local/export/<id>`.
import { readFileSync, realpathSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { CLI_CODES, INTERNAL_CODES, NORMALIZE_CODES } from './codes.ts';
import { kindAt } from './fsx.ts';
import {
  runNormalize,
  validateRecipe,
  type Issue as RecipeIssue,
  type Recipe,
} from './normalize.ts';
import { issue, line } from './report.ts';

const ROOT = fileURLToPath(new URL('../../../', import.meta.url));

export type Write = (text: string) => void;

const EXPORT_DIR = ['content', '_local', 'export'] as const;

/** `--out` is replaced, so neither folder may hold the other: the raw frames have no other copy. */
function overlaps(a: string, b: string): boolean {
  const inside = (parent: string, child: string): boolean => {
    const relative = path.relative(parent, child);
    return relative === '' || (!relative.startsWith('..') && !path.isAbsolute(relative));
  };
  return inside(a, b) || inside(b, a);
}

/**
 * The path as the disk spells it: `.native` also restores the case, which the plain `realpathSync`
 * keeps as written. What does not exist yet hangs from its nearest existing folder.
 */
function realPath(file: string): string {
  if (kindAt(file) !== undefined) return realpathSync.native(file);
  const parent = path.dirname(file);
  return parent === file ? file : path.join(realPath(parent), path.basename(file));
}

/**
 * The code that forbids replacing `out`, or `undefined` when it is free to. `out` is exactly
 * `<root>/content/_local/export/<id>`, no component from the root down is a symlink and it shares
 * no folder with the raw frames, even through a symlink or another case in `inDir`.
 */
export function checkOut(root: string, out: string, id: string, inDir: string): string | undefined {
  const relative = path.relative(root, out);
  const parts = relative === '' ? [] : relative.split(path.sep);
  if (parts.length !== EXPORT_DIR.length + 1 || !EXPORT_DIR.every((p, i) => parts[i] === p)) {
    return CLI_CODES.outForbidden;
  }
  if (parts.at(-1) !== id) return CLI_CODES.outIdMismatch;
  for (let i = 1; i <= parts.length; i++) {
    const kind = kindAt(path.join(root, ...parts.slice(0, i)));
    if (kind === undefined) break;
    if (kind === 'symlink') return CLI_CODES.outSymlink;
    if (kind !== 'dir') return CLI_CODES.outNotADirectory;
  }
  return overlaps(realPath(inDir), realPath(out)) ? CLI_CODES.outForbidden : undefined;
}

export function main(argv: readonly string[], env: NodeJS.ProcessEnv, write: Write): number {
  const usage = (code: string, data?: Readonly<Record<string, string>>): 2 => {
    write(line(issue(code, '', '', data)));
    return 2;
  };
  const base = env['INIT_CWD'] ?? process.cwd();
  const args = argv[0] === '--' ? argv.slice(1) : [...argv];
  let v: Record<string, string | undefined>;
  try {
    v = parseArgs({
      args,
      options: { in: { type: 'string' }, recipe: { type: 'string' }, out: { type: 'string' } },
      strict: true,
      allowPositionals: false,
    }).values;
  } catch (error) {
    const code = (error as { code?: unknown } | null)?.code;
    return usage(CLI_CODES.badArguments, { reason: typeof code === 'string' ? code : 'unknown' });
  }
  for (const arg of ['in', 'recipe', 'out']) {
    if (v[arg] === undefined || v[arg] === '') return usage(CLI_CODES.missingArgument, { arg });
  }
  const inDir = path.resolve(base, v['in'] as string);
  if (kindAt(inDir) !== 'dir') return usage(CLI_CODES.inNotADirectory);
  const outDir = path.resolve(base, v['out'] as string);
  let recipe: unknown;
  try {
    recipe = JSON.parse(readFileSync(path.resolve(base, v['recipe'] as string), 'utf8'));
  } catch {
    return usage(NORMALIZE_CODES.recipeInvalid);
  }
  const report = (issues: readonly RecipeIssue[]): 1 => {
    for (const i of issues) write(line(issue(i.code, '', i.path)));
    return 1;
  };
  const issues = validateRecipe(recipe);
  if (issues.length > 0) return report(issues);
  const valid = recipe as Recipe;
  const refused = checkOut(ROOT, outDir, valid.id, inDir);
  if (refused !== undefined) return usage(refused);
  const result = runNormalize({ inDir, recipe: valid, outDir });
  if (result.status === 1) return report(result.issues);
  write(JSON.stringify(result.summary));
  return 0;
}

if (
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const write: Write = (text) => process.stdout.write(text + '\n');
  try {
    process.exitCode = main(process.argv.slice(2), process.env, write);
  } catch {
    write(line(issue(INTERNAL_CODES.unexpected, '', '')));
    process.exitCode = 2;
  }
}
