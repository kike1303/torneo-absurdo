// CLI of the sprite importer (007 RF-B2, RF-B6, plan-b decisions 11 and 15). One JSON line per
// issue on stdout, then a summary when it passes. Exit 0 when it passes, 1 when the content does
// not meet the standard, 2 on a usage or I/O error; an unexpected exception is
// `internal.unexpected` with 2, never a stack nor a path. `--root` exists only in `check`, so the
// destination of `import` is always inside the real repository.
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { runCheck } from './check.ts';
import { CLI_CODES, INTERNAL_CODES } from './codes.ts';
import { kindAt } from './fsx.ts';
import { runImport } from './import.ts';
import { issue, line, type Issue } from './report.ts';

/** The repository root, from this file (tools/sprite-importer/src/cli.ts). */
const ROOT = fileURLToPath(new URL('../../../', import.meta.url));

export type Write = (text: string) => void;

function usage(write: Write, code: string, data?: Readonly<Record<string, string>>): 2 {
  write(line(issue(code, '', '', data)));
  return 2;
}

function parse(
  argv: readonly string[],
  options: Record<string, { type: 'string' }>,
): Record<string, string | undefined> | string {
  try {
    return parseArgs({ args: [...argv], options, strict: true, allowPositionals: false })
      .values as Record<string, string | undefined>;
  } catch (error) {
    const code = (error as { code?: unknown } | null)?.code;
    return typeof code === 'string' ? code : 'unknown';
  }
}

function emit(write: Write, issues: readonly Issue[]): void {
  for (const i of issues) write(line(i));
}

export function main(argv: readonly string[], env: NodeJS.ProcessEnv, write: Write): number {
  const base = env['INIT_CWD'] ?? process.cwd();
  const args = [...argv];
  if (args[0] === '--') args.shift();
  const command = args.shift();
  if (args[0] === '--') args.shift();
  if (command === 'check') {
    const v = parse(args, { root: { type: 'string' } });
    if (typeof v === 'string') return usage(write, CLI_CODES.badArguments, { reason: v });
    const root = v['root'] !== undefined && v['root'] !== '' ? path.resolve(base, v['root']) : ROOT;
    if (kindAt(root) !== 'dir') return usage(write, CLI_CODES.badArguments, { reason: 'root' });
    const result = runCheck({ root });
    emit(write, result.issues);
    if (result.summary !== undefined) write(line({ ...result.summary }));
    return result.status;
  }
  if (command === 'import') {
    const v = parse(args, { in: { type: 'string' }, out: { type: 'string' } });
    if (typeof v === 'string') return usage(write, CLI_CODES.badArguments, { reason: v });
    for (const arg of ['in', 'out']) {
      if (v[arg] === undefined || v[arg] === '') {
        return usage(write, CLI_CODES.missingArgument, { arg });
      }
    }
    const inDir = path.resolve(base, v['in'] as string);
    if (kindAt(inDir) !== 'dir') return usage(write, CLI_CODES.inNotADirectory);
    const result = runImport({ root: ROOT, inDir, outDir: path.resolve(base, v['out'] as string) });
    emit(write, result.issues);
    if (result.summary !== undefined) write(line({ ...result.summary }));
    return result.status;
  }
  return usage(write, CLI_CODES.unknownCommand);
}

function run(): void {
  const write: Write = (text) => process.stdout.write(text + '\n');
  try {
    process.exitCode = main(process.argv.slice(2), process.env, write);
  } catch {
    write(line(issue(INTERNAL_CODES.unexpected, '', '')));
    process.exitCode = 2;
  }
}

if (
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  run();
}
