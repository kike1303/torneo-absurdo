import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

// 002 RF-C1 (plan-c decision 8): one file knows both the engine log and the renderer timeline.
const SRC = fileURLToPath(new URL('../src/', import.meta.url));
const ADAPTER = path.join('lib', 'features', 'fight', 'timeline-from-fight-log.ts');
const PROTOCOL = /from\s+['"]@torneo\/protocol(?:\/[^'"]*)?['"]|import\(\s*['"]@torneo\/protocol/;
const RENDERER = /from\s+['"]@torneo\/renderer(?:\/[^'"]*)?['"]|import\(\s*['"]@torneo\/renderer/;

function sources(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) return entry.name === 'paraglide' ? [] : sources(full);
    return /\.(?:ts|svelte)$/.test(entry.name) ? [full] : [];
  });
}

describe('the FightLog → Timeline boundary', () => {
  it('only the adapter imports from both @torneo/protocol and @torneo/renderer', () => {
    const both = sources(SRC)
      .filter((file) => {
        const text = readFileSync(file, 'utf8');
        return PROTOCOL.test(text) && RENDERER.test(text);
      })
      .map((file) => path.relative(SRC, file));
    expect(both).toEqual([ADAPTER]);
  });
});
