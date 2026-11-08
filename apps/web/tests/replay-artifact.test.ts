import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { GOLDEN_DIR, goldenLogNames, type GoldenLogName } from './support/golden-logs.js';
import { deriveSeed } from '../e2e/support/fight-script.mjs';
import { realEngine, wasmBytes } from './support/replay-engine.js';

// 011 RF3: the committed binary behaves. It reproduces the goldens byte by byte, it does not carry
// the home of whoever generated it, and its surface is the two functions RF4 allows.

/** `VECTORS` of crates/engine/tests/golden.rs. */
const GOLDENS: Readonly<Record<GoldenLogName, { a: string; b: string; seed: number }>> = {
  'log-ko-decisive-crit': { a: 'trickster', b: 'tank', seed: 1 },
  'log-ko-knockdown': { a: 'striker', b: 'trickster', seed: 0 },
  'log-ko-comeback': { a: 'trickster', b: 'trickster', seed: 3 },
  'log-ko-clutch': { a: 'striker', b: 'tank', seed: 1 },
  'log-decision-a': { a: 'tank', b: 'turtle', seed: 0 },
  'log-decision-b': { a: 'tank', b: 'tank', seed: 0 },
  'log-draw': { a: 'turtle', b: 'turtle', seed: 118 },
};

const noFinalNewline = (file: string) =>
  readFileSync(`${GOLDEN_DIR}${file}`, 'utf8').replace(/\n$/, '');

describe('the committed engine wasm', () => {
  it('has a vector for every golden', () => {
    expect(Object.keys(GOLDENS).sort()).toEqual(goldenLogNames());
    expect(Object.keys(GOLDENS)).toHaveLength(7);
  });

  it.each(Object.entries(GOLDENS))('reproduces %s byte by byte', (name, { a, b, seed }) => {
    const engine = realEngine();
    const log = engine.simulate(
      deriveSeed(seed),
      noFinalNewline(`spec-${a}.json`),
      noFinalNewline(`spec-${b}.json`),
      1,
    );
    expect(log).toBe(noFinalNewline(`${name}.json`));
  });

  it('gives ruleset.unknown with an unknown version, from both functions', () => {
    const engine = realEngine();
    const spec = noFinalNewline('spec-tank.json');
    expect(() => engine.simulate(deriveSeed(0), spec, spec, 3)).toThrow('ruleset.unknown');
    expect(() => engine.rulesetBounds(3)).toThrow('ruleset.unknown');
  });

  it('gives the bounds of the ruleset', () => {
    expect(JSON.parse(realEngine().rulesetBounds(1))).toEqual({
      time_limit_ms: 86_000,
      max_duration_ms: 90_000,
    });
  });

  it('does not carry the path of a home', () => {
    const text = wasmBytes().toString('latin1');
    expect(text).not.toContain('/Users/');
    expect(text).not.toContain('/home/');
  });

  it('exports only its two functions, and imports only from the glue', () => {
    const module = new WebAssembly.Module(new Uint8Array(wasmBytes()));
    const own = WebAssembly.Module.exports(module)
      .filter((entry) => entry.kind === 'function' && !entry.name.startsWith('__wbindgen'))
      .map((entry) => entry.name)
      .sort();
    expect(own).toEqual(['ruleset_bounds', 'simulate']);
    const imports = WebAssembly.Module.imports(module);
    expect(imports.length).toBeGreaterThan(0);
    for (const entry of imports) {
      expect(entry.module).toMatch(/^(?:wbg|\.\/engine_wasm_bg\.js)$/);
      expect(entry.name).toMatch(/^__wb(?:g|indgen)_/);
    }
  });
});
