// CLI of the converter (002 RF-B2, plan-b decision 11). Prints JSON without paths: stdout gets
// the report, stderr `{ "error": code, "data": … }`. Exit 1 on a ConverterError or an invalid
// manifest, 2 on anything else (`internal.unexpected`, never a stack nor a path).
import { readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { convert } from './convert.ts';
import { ConverterError, fail, type ErrorData } from './errors.ts';
import { inspect } from './inspect.ts';
import { INPUT_LIMITS } from './limits.ts';
import { parseMap, type CharacterMap } from './map.ts';
import { writeOutput } from './write.ts';

type InputArg = 'sff' | 'air' | 'map';
const LIMIT: Readonly<Record<InputArg, number>> = {
  sff: INPUT_LIMITS.sffBytes,
  air: INPUT_LIMITS.airBytes,
  map: INPUT_LIMITS.mapBytes,
};

/** The repository root, from this file (tools/sff-converter/src/cli.ts). */
const ROOT = fileURLToPath(new URL('../../../', import.meta.url));

function base(): string {
  return process.env['INIT_CWD'] ?? process.cwd();
}

function readInput(arg: InputArg, value: string | undefined): Uint8Array {
  if (value === undefined || value === '') return fail('cli.missing_argument', { arg });
  const file = path.resolve(base(), value);
  let size: number;
  try {
    const stat = statSync(file);
    if (!stat.isFile()) return fail('cli.not_a_file', { arg });
    size = stat.size;
  } catch (error) {
    if (error instanceof ConverterError) throw error;
    const code = (error as { code?: unknown } | null)?.code;
    if (code === 'ENOENT' || code === 'ENOTDIR') return fail('cli.file_not_found', { arg });
    if (code === 'EACCES' || code === 'EPERM') return fail('cli.file_unreadable', { arg });
    throw error;
  }
  if (size > LIMIT[arg]) return fail('cli.file_too_large', { arg, limit: LIMIT[arg] });
  try {
    return readFileSync(file);
  } catch (error) {
    const code = (error as { code?: unknown } | null)?.code;
    if (code === 'EACCES' || code === 'EPERM') return fail('cli.file_unreadable', { arg });
    throw error;
  }
}

function readMap(value: string | undefined): CharacterMap {
  const text = Buffer.from(readInput('map', value)).toString('utf8');
  let json: unknown;
  try {
    json = JSON.parse(text);
  } catch {
    return fail('map.invalid_json');
  }
  return parseMap(json);
}

function print(stream: NodeJS.WriteStream, value: unknown): void {
  stream.write(JSON.stringify(value, null, 2) + '\n');
}

function parse(argv: readonly string[], options: Record<string, { type: 'string' }>) {
  try {
    return parseArgs({ args: [...argv], options, strict: true, allowPositionals: false })
      .values as Record<string, string | undefined>;
  } catch (error) {
    const code = (error as { code?: unknown } | null)?.code;
    return fail('cli.bad_arguments', { reason: typeof code === 'string' ? code : 'unknown' });
  }
}

export function main(argv: readonly string[]): number {
  const args = [...argv];
  if (args[0] === '--') args.shift();
  const command = args.shift();
  if (args[0] === '--') args.shift();
  if (command === 'convert') {
    const v = parse(args, {
      sff: { type: 'string' },
      air: { type: 'string' },
      map: { type: 'string' },
      out: { type: 'string' },
    });
    const sff = readInput('sff', v['sff']);
    const air = readInput('air', v['air']);
    const map = readMap(v['map']);
    const result = convert({ sff, air, map });
    if (!result.ok) {
      print(process.stdout, result.report);
      print(process.stderr, {
        error: 'convert.invalid_manifest',
        data: { errors: result.errors.length },
      });
      return 1;
    }
    const out =
      v['out'] !== undefined && v['out'] !== ''
        ? path.resolve(base(), v['out'])
        : path.join(ROOT, 'content', '_local', 'sprites', map.id);
    writeOutput(out, result.manifest, result.sheets);
    print(process.stdout, result.report);
    return 0;
  }
  if (command === 'inspect') {
    const v = parse(args, {
      sff: { type: 'string' },
      air: { type: 'string' },
      map: { type: 'string' },
    });
    const report = inspect({
      sff: readInput('sff', v['sff']),
      ...(v['air'] !== undefined ? { air: readInput('air', v['air']) } : {}),
      ...(v['map'] !== undefined ? { map: readMap(v['map']) } : {}),
    });
    print(process.stdout, report);
    return 0;
  }
  return fail('cli.unknown_command');
}

function run(): void {
  let status: number;
  try {
    status = main(process.argv.slice(2));
  } catch (error) {
    if (error instanceof ConverterError) {
      const data: ErrorData = error.data;
      print(process.stderr, { error: error.code, data });
      status = 1;
    } else {
      print(process.stderr, { error: 'internal.unexpected', data: {} });
      status = 2;
    }
  }
  process.exitCode = status;
}

if (
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  run();
}
