// The real `.wasm` in Node, the same bytes the browser gets (011 RF3, RF11, RF14). Tests are not bound
// by the rule of `engine-loader.ts`: they load the committed file with `initSync`.
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import {
  initSync,
  ruleset_bounds,
  simulate,
} from '../../src/lib/features/replay/engine-wasm/engine_wasm.js';
import type { VerifyDeps, VerifyEngine } from '../../src/lib/features/replay/verify.js';

export const WASM_FILE = fileURLToPath(
  new URL('../../src/lib/features/replay/engine-wasm/engine_wasm_bg.wasm', import.meta.url),
);
export const wasmBytes = (): Buffer => readFileSync(WASM_FILE);

let ready = false;

export function realEngine(): VerifyEngine {
  if (!ready) {
    initSync({ module: wasmBytes() });
    ready = true;
  }
  return { simulate, rulesetBounds: ruleset_bounds };
}

export function realDeps(overrides: Partial<VerifyDeps> = {}): VerifyDeps {
  return {
    sha256: async (bytes) => new Uint8Array(createHash('sha256').update(bytes).digest()),
    engine: async () => realEngine(),
    ...overrides,
  };
}
