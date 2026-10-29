import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

// 007 RF-A2: the runtime of the renderer never loads the guide, and the guide never loads the
// runtime nor Pixi. Walks the static imports of the source (check-renderer-split only sees the web
// build, which never imports the guide, so it would pass vacuously).
const ROOT = fileURLToPath(new URL('..', import.meta.url));
const src = (file: string) => path.join(ROOT, 'src', file);

const STATIC_IMPORT =
  /(?:^|[\s;])(?:import|export)\s+(?:type\s+)?(?:[^'";]*?\s+from\s+)?['"]([^'"]+)['"]/g;

function specifiers(file: string): string[] {
  const code = readFileSync(file, 'utf8').replace(/\/\*[\s\S]*?\*\/|\/\/[^\n]*/g, '');
  return [...code.matchAll(STATIC_IMPORT)].map((m) => m[1] ?? '');
}

/** Every file reachable by relative static imports from `entry`, and the bare specifiers seen. */
function closure(entry: string): { files: Set<string>; packages: Set<string> } {
  const files = new Set<string>();
  const packages = new Set<string>();
  const queue = [entry];
  while (queue.length > 0) {
    const file = queue.pop() as string;
    if (files.has(file)) continue;
    files.add(file);
    for (const spec of specifiers(file)) {
      if (spec.startsWith('.')) queue.push(path.resolve(path.dirname(file), spec));
      else packages.add(spec);
    }
  }
  return { files, packages };
}

const rel = (file: string) => path.relative(ROOT, file).split(path.sep).join('/');
const under = (files: Set<string>, dir: string) =>
  [...files].map(rel).filter((f) => f.startsWith(`src/${dir}/`));

describe('the guide is outside the runtime', () => {
  it('finds the imports it walks (the walker is not vacuous)', () => {
    const { files, packages } = closure(src('stage/index.ts'));
    expect(under(files, 'scene').length).toBeGreaterThan(0);
    expect([...packages]).toContain('pixi.js');
    expect(under(closure(src('guide/index.ts')).files, 'manifest').length).toBeGreaterThan(0);
  });

  it.each(['index.ts', 'stage/index.ts'])('src/%s never reaches src/guide/', (entry) => {
    expect(under(closure(src(entry)).files, 'guide')).toEqual([]);
  });

  it('src/guide/index.ts never reaches stage, player nor pixi.js', () => {
    const { files, packages } = closure(src('guide/index.ts'));
    expect(under(files, 'stage')).toEqual([]);
    expect(under(files, 'player')).toEqual([]);
    expect([...packages].filter((p) => p === 'pixi.js' || p.startsWith('pixi.js/'))).toEqual([]);
  });

  it('package.json exposes the ./guide subpath', () => {
    const pkg = JSON.parse(readFileSync(path.join(ROOT, 'package.json'), 'utf8')) as {
      exports: Record<string, unknown>;
    };
    expect(pkg.exports['./guide']).toEqual({
      types: './src/guide/index.ts',
      default: './src/guide/index.ts',
    });
  });
});
