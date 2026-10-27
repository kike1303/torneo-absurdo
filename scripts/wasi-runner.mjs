#!/usr/bin/env node
// 003 RF18: cargo runner for wasm32-wasip1 (`CARGO_TARGET_WASM32_WASIP1_RUNNER`, set by
// `just _check-wasm`), so the engine goldens run in WASM with node:wasi, without wasmtime.
// No preopens (no file system) and an empty environment: the goldens are `include_bytes!`, and
// UPDATE_GOLDEN never reaches the module. A trap (`panic = abort` reaches `unreachable`) is
// exit code 101, the same code a failed native test binary returns.
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { WASI } from 'node:wasi';

export const TRAP_EXIT_CODE = 101;

/**
 * @param {string} wasmPath
 * @param {readonly string[]} args the arguments after the module (cargo passes the test filters)
 */
export function wasiOptions(wasmPath, args) {
  return {
    version: /** @type {const} */ ('preview1'),
    args: [path.basename(wasmPath), ...args],
    env: {},
    returnOnExit: true,
  };
}

/**
 * Runs a WASI command module and returns its exit code.
 * @param {string} wasmPath
 * @param {readonly string[]} args
 * @param {{ write(message: string): unknown }} [stderr]
 * @returns {Promise<number>}
 */
export async function runWasi(wasmPath, args, stderr = process.stderr) {
  let module;
  try {
    module = await WebAssembly.compile(await readFile(wasmPath));
  } catch (error) {
    stderr.write(`wasi-runner: cannot load ${wasmPath}: ${String(error)}\n`);
    return 1;
  }
  const wasi = new WASI(wasiOptions(wasmPath, args));
  try {
    const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
    return wasi.start(instance) ?? 0;
  } catch (error) {
    if (error instanceof WebAssembly.RuntimeError) {
      stderr.write(`wasi-runner: trap: ${error.message}\n`);
      return TRAP_EXIT_CODE;
    }
    stderr.write(`wasi-runner: ${String(error)}\n`);
    return 1;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [wasmPath, ...args] = process.argv.slice(2);
  if (!wasmPath) {
    console.error('usage: wasi-runner.mjs <module.wasm> [args...]');
    process.exit(1);
  }
  process.exit(await runWasi(wasmPath, args));
}
