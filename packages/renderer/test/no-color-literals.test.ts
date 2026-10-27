import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

// RF-A12 and plan-a1 decision 9: no color literals in src/, except the neutral tint of A2.
const SRC = fileURLToPath(new URL('../src/', import.meta.url));
const ALLOWED = new Set(['stage/neutral.ts']);

const PATTERNS: readonly RegExp[] = [
  /\b0x[0-9a-f]{6}\b/i,
  /(?<![\w$.])#(?:[0-9a-f]{8}|[0-9a-f]{6}|[0-9a-f]{3,4})\b/i,
  /\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch)\s*\(/i,
];

export function colorLiterals(file: string, text: string): string[] {
  if (ALLOWED.has(file)) return [];
  const found: string[] = [];
  text.split('\n').forEach((line, i) => {
    for (const pattern of PATTERNS) {
      const match = pattern.exec(line);
      if (match) found.push(`${file}:${i + 1}: ${match[0]}`);
    }
  });
  return found;
}

function sources(dir: string, base = ''): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const rel = base ? `${base}/${entry.name}` : entry.name;
    if (entry.isDirectory()) return sources(path.join(dir, entry.name), rel);
    return entry.name.endsWith('.ts') ? [rel] : [];
  });
}

describe('no color literals in packages/renderer/src', () => {
  it('src/ has none', () => {
    const found = sources(SRC).flatMap((file) =>
      colorLiterals(file, readFileSync(path.join(SRC, file), 'utf8')),
    );
    expect(found).toEqual([]);
  });

  it.each([
    ['0xff00aa', 'export const c = 0xff00aa;'],
    ['#rgb', "export const c = '#f0a';"],
    ['#rrggbbaa', "export const c = '#ff00aa80';"],
    ['rgb()', "export const c = 'rgb(255 0 170)';"],
    ['oklch()', "export const c = 'oklch(0.7 0.2 330)';"],
  ])('detects %s', (_name, code) => {
    expect(colorLiterals('scene/x.ts', code)).toHaveLength(1);
  });

  it('allows the neutral tint file of the Pixi layer', () => {
    expect(colorLiterals('stage/neutral.ts', 'export const WHITE = 0xffffff;')).toEqual([]);
  });

  it('ignores 32-bit constants that are not colors', () => {
    expect(colorLiterals('fx/x.ts', 'const k = 0x6d2b79f5;')).toEqual([]);
  });
});
