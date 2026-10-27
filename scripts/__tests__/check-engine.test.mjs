import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import {
  checkEngineGraph,
  checkWasmLints,
  checkWasmSources,
  parseCargoTree,
  parseLintTables,
} from '../check-engine.mjs';

// Real `cargo tree -p engine -e normal,features --target all --prefix none` (paths shortened).
const TREE_OK = readFileSync(
  new URL('../__fixtures__/check-engine/tree-ok.txt', import.meta.url),
  'utf8',
);
const WORKSPACE_TOML = readFileSync(new URL('../../Cargo.toml', import.meta.url), 'utf8');
const WASM_TOML = readFileSync(
  new URL('../../crates/engine-wasm/Cargo.toml', import.meta.url),
  'utf8',
);

describe('parseCargoTree', () => {
  it('reads packages and features, with or without (*) and (proc-macro)', () => {
    const tree = parseCargoTree(
      [
        'engine v0.1.0 (/repo/crates/engine)',
        'rand_core feature "default"',
        'rand_core v0.9.5',
        'serde_derive v1.0.229 (proc-macro)',
        'proc-macro2 feature "proc-macro" (*)',
        '',
      ].join('\n'),
    );
    expect([...tree.packages]).toEqual(['engine', 'rand_core', 'serde_derive']);
    expect([...(tree.features.get('rand_core') ?? [])]).toEqual(['default']);
    expect([...(tree.features.get('proc-macro2') ?? [])]).toEqual(['proc-macro']);
  });
  it('throws on a line it does not understand (a tree with prefixes)', () => {
    expect(() => parseCargoTree('├── rand_core v0.9.5')).toThrow(/unexpected line/);
  });
});

describe('checkEngineGraph (RF5, RF20)', () => {
  it('passes with the real graph of the engine', () => {
    expect(checkEngineGraph(parseCargoTree(TREE_OK))).toEqual([]);
  });
  it.each([
    'getrandom v0.3.4',
    'rand v0.9.2',
    'tokio v1.54.1',
    'ts-rs v12.0.1',
    'tracing v0.1.44',
    'log v0.4.29',
  ])('fails with %s in the graph', (line) => {
    const name = line.split(' ')[0];
    expect(checkEngineGraph(parseCargoTree(`${TREE_OK}${line}\n`))).toEqual([
      `${name}: in the normal graph of engine (RF5)`,
    ]);
  });
  it.each([
    ['rand_core', 'os_rng'],
    ['rand_core', 'std'],
    ['rand_core', 'serde'],
    ['rand_chacha', 'serde'],
    ['rand_chacha', 'std'],
    ['rand_chacha', 'os_rng'],
  ])('fails with %s feature "%s"', (name, flag) => {
    expect(checkEngineGraph(parseCargoTree(`${TREE_OK}${name} feature "${flag}"\n`))).toEqual([
      `${name}: feature "${flag}" is enabled in the graph of engine (RF5)`,
    ]);
  });
  it('does not mistake rand_chacha or rand_core for rand', () => {
    const tree = parseCargoTree('engine v0.1.0\nrand_chacha v0.9.0\nrand_core v0.9.5\n');
    expect(checkEngineGraph(tree)).toEqual([]);
  });
  it('allows other features of other crates (serde "std")', () => {
    expect(TREE_OK).toMatch(/^serde feature "std"$/m);
    expect(checkEngineGraph(parseCargoTree(TREE_OK))).toEqual([]);
  });
  it('fails if the output does not contain the engine', () => {
    expect(checkEngineGraph(parseCargoTree('rand_core v0.9.5\n'))).toEqual([
      'engine: not in the cargo tree output',
    ]);
  });
});

describe('checkWasmSources (RF19)', () => {
  const allowed = '#[allow(unsafe_code)]\n#[wasm_bindgen]\npub fn simulate() {}\n';
  it('passes with a single allow(unsafe_code) and no unsafe token', () => {
    expect(checkWasmSources([{ file: 'lib.rs', text: allowed }])).toEqual([]);
  });
  it('passes with the real sources', () => {
    const text = readFileSync(
      new URL('../../crates/engine-wasm/src/lib.rs', import.meta.url),
      'utf8',
    );
    expect(checkWasmSources([{ file: 'lib.rs', text }])).toEqual([]);
  });
  it('fails with the token unsafe (code or comment)', () => {
    expect(checkWasmSources([{ file: 'lib.rs', text: `${allowed}unsafe { x() }\n` }])).toEqual([
      'lib.rs: contains the token `unsafe` 1 time(s) (RF19)',
    ]);
    expect(checkWasmSources([{ file: 'a.rs', text: '// this is unsafe\n' }])).toHaveLength(1);
  });
  it('fails with two allow(unsafe_code), also across files', () => {
    expect(checkWasmSources([{ file: 'lib.rs', text: allowed + allowed }])).toEqual([
      'crates/engine-wasm/src: 2 allow(unsafe_code), at most 1 (RF19)',
    ]);
    expect(
      checkWasmSources([
        { file: 'a.rs', text: allowed },
        { file: 'b.rs', text: '#[allow( unsafe_code )]\n' },
      ]),
    ).toHaveLength(1);
  });
});

describe('parseLintTables', () => {
  it('reads the workspace lint tables', () => {
    const tables = parseLintTables(WORKSPACE_TOML, 'workspace.lints');
    expect(tables.rust).toEqual({ unsafe_code: 'forbid' });
    expect(tables.clippy?.unwrap_used).toBe('deny');
  });
  it('ignores comments and other tables', () => {
    const toml =
      '[package]\nname = "x"\n[lints.rust]\nunsafe_code = "deny" # why\n[dependencies]\na = "1"\n';
    expect(parseLintTables(toml, 'lints')).toEqual({ rust: { unsafe_code: 'deny' } });
  });
});

describe('checkWasmLints (RF19)', () => {
  const workspace =
    '[workspace.lints.rust]\nunsafe_code = "forbid"\n\n[workspace.lints.clippy]\nunwrap_used = "deny"\nprint_stdout = "deny"\n';
  const wasm =
    '[lints.rust]\nunsafe_code = "deny"\n\n[lints.clippy]\nunwrap_used = "deny"\nprint_stdout = "deny"\n';
  it('passes with the real files', () => {
    expect(checkWasmLints(WORKSPACE_TOML, WASM_TOML)).toEqual([]);
  });
  it('passes when only unsafe_code goes from forbid to deny', () => {
    expect(checkWasmLints(workspace, wasm)).toEqual([]);
  });
  it('fails with a different lint level', () => {
    expect(
      checkWasmLints(workspace, wasm.replace('unwrap_used = "deny"', 'unwrap_used = "warn"')),
    ).toEqual([
      'crates/engine-wasm/Cargo.toml: [lints.clippy] unwrap_used = warn, expected deny (RF19)',
    ]);
  });
  it('fails with a missing lint', () => {
    expect(checkWasmLints(workspace, wasm.replace('print_stdout = "deny"\n', ''))).toEqual([
      'crates/engine-wasm/Cargo.toml: [lints.clippy] print_stdout = (missing), expected deny (RF19)',
    ]);
  });
  it('fails with an extra lint', () => {
    expect(checkWasmLints(workspace, `${wasm}dbg_macro = "allow"\n`)).toEqual([
      'crates/engine-wasm/Cargo.toml: [lints.clippy] dbg_macro = allow, expected (absent) (RF19)',
    ]);
  });
  it('fails if unsafe_code is allow or forbid in engine-wasm', () => {
    expect(
      checkWasmLints(workspace, wasm.replace('unsafe_code = "deny"', 'unsafe_code = "allow"')),
    ).toHaveLength(1);
    expect(
      checkWasmLints(workspace, wasm.replace('unsafe_code = "deny"', 'unsafe_code = "forbid"')),
    ).toHaveLength(1);
  });
  it('fails with `[lints] workspace = true` (no explicit table)', () => {
    expect(checkWasmLints(workspace, '[lints]\nworkspace = true\n').length).toBeGreaterThan(0);
  });
});
