import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { LIMITS, REACTION_WINDOW_MS, validateManifest } from '../src/manifest/index.ts';
import { LOOKAHEAD_MS, MAX_WINDUP_MS } from '../src/scene/index.ts';
import { TIMELINE_LIMITS } from '../src/timeline/index.ts';
import { applyOps, loadFixtures } from './support/patch.ts';

// 007 RF-A1: the windup cap derives from the timeline rules instead of a literal.
describe('the windup cap is derived', () => {
  it('REACTION_WINDOW_MS is the 200 ms guarantee of the choreography', () => {
    expect(REACTION_WINDOW_MS).toBe(200);
  });

  it('LIMITS.windupMaxMs = strikeGapMs − REACTION_WINDOW_MS', () => {
    expect(LIMITS.windupMaxMs).toBe(TIMELINE_LIMITS.strikeGapMs - REACTION_WINDOW_MS);
  });

  it('keeps the value at 400 in phase A (a change must revisit the look-ahead, ADR-0002)', () => {
    expect(LIMITS.windupMaxMs).toBe(400);
  });

  it('the choreography and the look-ahead read it (and no import cycle leaves it undefined)', () => {
    expect(MAX_WINDUP_MS).toBeDefined();
    expect(MAX_WINDUP_MS).toBe(LIMITS.windupMaxMs);
    expect(LOOKAHEAD_MS).toBe(MAX_WINDUP_MS);
  });

  it('choreography.ts has no hand-written windup literal', () => {
    const source = readFileSync(new URL('../src/scene/choreography.ts', import.meta.url), 'utf8');
    expect(source).not.toMatch(/=\s*400\b/);
  });

  it('a 401 ms windup still fails validateManifest', () => {
    const dir = fileURLToPath(new URL('./fixtures/manifest/', import.meta.url));
    const { valid, rules } = loadFixtures(dir);
    const rule = rules.find((r) => r.name === 'windup_too_long');
    expect(rule).toBeDefined();
    if (!rule) return;
    const result = validateManifest(applyOps(valid, rule.ops), rule.options);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.errors.map((e) => e.code)).toContain('manifest.windup_too_long');
  });
});
