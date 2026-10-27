import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterAll, describe, expect, it } from 'vitest';
import { TRAP_EXIT_CODE, runWasi, wasiOptions } from '../wasi-runner.mjs';

// Minimal WASI command modules, written as bytes (no committed binaries). Each one imports
// proc_exit, exports `memory` and a `_start` whose body is `code` (a function body: locals
// vector, instructions, `end`).
const name = (text) => [text.length, ...Buffer.from(text, 'latin1')];
const section = (id, bytes) => [id, bytes.length, ...bytes];
function wasiModule(code) {
  return Uint8Array.from([
    ...[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
    // types: 0 = (i32) -> (), 1 = () -> ()
    ...section(1, [0x02, 0x60, 0x01, 0x7f, 0x00, 0x60, 0x00, 0x00]),
    ...section(2, [0x01, ...name('wasi_snapshot_preview1'), ...name('proc_exit'), 0x00, 0x00]),
    ...section(3, [0x01, 0x01]),
    ...section(5, [0x01, 0x00, 0x01]),
    ...section(7, [0x02, ...name('memory'), 0x02, 0x00, ...name('_start'), 0x00, 0x01]),
    ...section(10, [0x01, code.length, ...code]),
  ]);
}
const PROC_EXIT_3 = wasiModule([0x00, 0x41, 0x03, 0x10, 0x00, 0x0b]); // i32.const 3; call 0
const UNREACHABLE = wasiModule([0x00, 0x00, 0x0b]);
const RETURNS = wasiModule([0x00, 0x0b]);

const dir = mkdtempSync(path.join(tmpdir(), 'wasi-runner-'));
afterAll(() => rmSync(dir, { recursive: true, force: true }));
function file(fileName, bytes) {
  const target = path.join(dir, fileName);
  writeFileSync(target, bytes);
  return target;
}
function sink() {
  const lines = [];
  return { lines, write: (message) => lines.push(message) };
}

describe('wasiOptions (RF18)', () => {
  it('has an empty environment, no preopens and the module name as args[0]', () => {
    const options = wasiOptions('/x/target/golden-123.wasm', ['--exact', 'a']);
    expect(options).toEqual({
      version: 'preview1',
      args: ['golden-123.wasm', '--exact', 'a'],
      env: {},
      returnOnExit: true,
    });
    expect(options).not.toHaveProperty('preopens');
  });
});

describe('runWasi (RF18)', () => {
  it('returns the code of proc_exit', async () => {
    expect(await runWasi(file('exit3.wasm', PROC_EXIT_3), [], sink())).toBe(3);
  });
  it('turns a trap into 101', async () => {
    const err = sink();
    expect(await runWasi(file('trap.wasm', UNREACHABLE), [], err)).toBe(TRAP_EXIT_CODE);
    expect(TRAP_EXIT_CODE).toBe(101);
    expect(err.lines.join('')).toMatch(/^wasi-runner: trap: /);
  });
  it('returns 0 when _start returns', async () => {
    expect(await runWasi(file('ok.wasm', RETURNS), [], sink())).toBe(0);
  });
  it('returns 1 with a message for a file that is not wasm', async () => {
    const err = sink();
    expect(await runWasi(file('not.wasm', Buffer.from('hola')), [], err)).toBe(1);
    expect(err.lines.join('')).toMatch(/^wasi-runner: cannot load .*not\.wasm/);
  });
});
