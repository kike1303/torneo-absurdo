#!/usr/bin/env node
// The script of `FIGHT_SCRIPT_PATH` for the e2e with real fights (010 RF45, plan-d decision 16): the
// vectors of two engine goldens, a KO and a draw, built from the golden specs and the seed of each
// vector, so regenerating the goldens keeps it valid. It is written, never committed.
//
//   node apps/web/e2e/support/fight-script.mjs
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const WEB = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const GOLDEN = path.resolve(WEB, '../../crates/engine/tests/golden');
export const FIGHT_SCRIPT = path.join(WEB, 'test-results', 'fight-script.json');

/** `crates/engine/tests/common/mod.rs`. */
const ROOT_SEED = 'torneo-absurdo-engine-golden-v1!';

/** The vectors of `crates/engine/tests/golden.rs`, by log. */
export const VECTORS = {
  ko: { log: 'log-ko-knockdown.json', a: 'striker', b: 'trickster', seed: 0 },
  draw: { log: 'log-draw.json', a: 'turtle', b: 'turtle', seed: 118 },
};

/** `derive_seed`: the root with the index in XOR, little endian. */
export function deriveSeed(/** @type {number} */ index) {
  const seed = Buffer.from(ROOT_SEED, 'ascii');
  for (let i = 0; i < 4; i++) seed.writeUInt8(seed.readUInt8(i) ^ ((index >>> (8 * i)) & 0xff), i);
  return seed.toString('hex');
}

const spec = (/** @type {string} */ name) =>
  JSON.parse(readFileSync(path.join(GOLDEN, `spec-${name}.json`), 'utf8'));

export function fightScript() {
  return Object.values(VECTORS).map(({ a, b, seed }) => ({
    seed: deriveSeed(seed),
    spec_a: spec(a),
    spec_b: spec(b),
  }));
}

/** The exact bytes of a golden, without the final newline of the file. */
export function goldenLog(/** @type {keyof typeof VECTORS} */ name) {
  return readFileSync(path.join(GOLDEN, VECTORS[name].log), 'utf8').replace(/\n$/, '');
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  mkdirSync(path.dirname(FIGHT_SCRIPT), { recursive: true });
  writeFileSync(FIGHT_SCRIPT, JSON.stringify(fightScript()));
}
