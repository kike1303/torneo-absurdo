import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import {
  NOT_ENGINE_CONSUMERS,
  checkEngineGraph,
  checkServerGraph,
  engineConsumers,
  checkWasmLints,
  checkWasmSources,
  parseCargoTree,
  parseLintTables,
  stripComments,
} from '../check-engine.mjs';

// Real `cargo tree -p engine -e normal,features --target all --prefix none` (paths shortened).
const TREE_OK = readFileSync(
  new URL('../__fixtures__/check-engine/tree-ok.txt', import.meta.url),
  'utf8',
);
// Real `cargo tree -p content -p engine-wasm -p sim -e normal,features --target all --prefix none`
// (paths shortened).
const TREE_CONSUMERS_OK = readFileSync(
  new URL('../__fixtures__/check-engine/tree-consumers-ok.txt', import.meta.url),
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

// The shape of `cargo metadata --format-version 1 --no-deps` for this workspace (only the fields
// engineConsumers reads; `path` is present only on path dependencies).
const dep = (name, kind = null, path = true) => ({
  name,
  kind,
  ...(path ? { path: `/repo/${name}` } : {}),
});
const pkg = (name, dependencies = []) => ({ name, dependencies });
const WORKSPACE = [
  pkg('server', [dep('axum', null, false), dep('protocol'), dep('tokio', 'dev', false)]),
  pkg('protocol', [dep('serde', null, false)]),
  pkg('content', [dep('engine'), dep('serde', null, false)]),
  pkg('engine', [dep('protocol'), dep('rand_chacha', null, false)]),
  pkg('engine-wasm', [dep('engine'), dep('wasm-bindgen', null, false)]),
  pkg('sim', [dep('content'), dep('engine'), dep('rand_chacha', null, false)]),
];
const CONSUMERS = ['content', 'engine-wasm', 'sim'];

describe('engineConsumers (plan-b decision 13, from cargo metadata)', () => {
  it('finds content, engine-wasm and sim in the real workspace shape', () => {
    expect(engineConsumers({ packages: WORKSPACE })).toEqual(CONSUMERS);
  });
  it('picks up a new member that depends on engine, so it cannot be forgotten', () => {
    const packages = [...WORKSPACE, pkg('narrator', [dep('engine')])];
    expect(engineConsumers({ packages })).toEqual(['content', 'engine-wasm', 'narrator', 'sim']);
  });
  it('picks up a member that reaches engine only through another member', () => {
    const packages = [...WORKSPACE, pkg('arena', [dep('content')])];
    expect(engineConsumers({ packages })).toContain('arena');
  });
  it('counts build dependencies but not dev dependencies', () => {
    const packages = [
      ...WORKSPACE,
      pkg('codegen', [dep('engine', 'build')]),
      pkg('bench', [dep('engine', 'dev')]),
    ];
    expect(engineConsumers({ packages })).toEqual(['codegen', 'content', 'engine-wasm', 'sim']);
  });
  it('does not mistake a crates.io package named like a member for the member', () => {
    const packages = [...WORKSPACE, pkg('other', [dep('engine', null, false)])];
    expect(engineConsumers({ packages })).toEqual(CONSUMERS);
  });
  it('leaves out the server even when it depends on engine', () => {
    expect(NOT_ENGINE_CONSUMERS).toEqual(['server']);
    const packages = WORKSPACE.map((p) =>
      p.name === 'server' ? pkg('server', [...p.dependencies, dep('engine')]) : p,
    );
    expect(engineConsumers({ packages })).toEqual(CONSUMERS);
  });
  it('throws if engine is not a workspace member', () => {
    expect(() => engineConsumers({ packages: [pkg('content')] })).toThrow(/engine/);
  });
});

describe('checkEngineGraph over the consumers (plan-b decision 13)', () => {
  const check = (text) => checkEngineGraph(parseCargoTree(text), { roots: CONSUMERS });
  const graph = 'content and engine-wasm and sim';
  it('passes with the real graph of the consumers, which contains the engine', () => {
    expect(TREE_CONSUMERS_OK).toMatch(/^engine v0\.1\.0 /m);
    expect(TREE_CONSUMERS_OK).toMatch(/\(command-line\)$/m);
    expect(TREE_CONSUMERS_OK).toMatch(/\(\*\)$/m);
    expect(check(TREE_CONSUMERS_OK)).toEqual([]);
  });
  it.each([
    ['rand_chacha', 'std'],
    ['rand_chacha', 'serde'],
    ['rand_core', 'os_rng'],
  ])('fails with %s feature "%s", naming the consumers', (name, flag) => {
    expect(check(`${TREE_CONSUMERS_OK}${name} feature "${flag}"\n`)).toEqual([
      `${name}: feature "${flag}" is enabled in the graph of ${graph} (RF5)`,
    ]);
  });
  it.each(['getrandom v0.3.4', 'rand v0.9.2', 'tracing v0.1.44', 'log v0.4.29'])(
    'fails with %s in the graph',
    (line) => {
      const name = line.split(' ')[0];
      expect(check(`${TREE_CONSUMERS_OK}${line}\n`)).toEqual([
        `${name}: in the normal graph of ${graph} (RF5)`,
      ]);
    },
  );
  it('fails if a consumer is missing from the output', () => {
    expect(
      check('content v0.1.0 (/repo/crates/content)\nengine-wasm v0.1.0\nengine v0.1.0\n'),
    ).toEqual(['sim: not in the cargo tree output']);
  });
});

describe('checkServerGraph (ADR-0002)', () => {
  const SERVER = 'server v0.1.0 (/repo/apps/server)\nprotocol v0.1.0 (/repo/crates/protocol)\n';
  it('passes when sim is not in the graph of the server', () => {
    expect(checkServerGraph(parseCargoTree(SERVER))).toEqual([]);
  });
  it('fails when the server depends on sim, directly or not', () => {
    expect(checkServerGraph(parseCargoTree(`${SERVER}sim v0.1.0 (/repo/crates/sim)\n`))).toEqual([
      'sim: in the dependency graph of server (ADR-0002: its seeds are public)',
    ]);
  });
  it('fails if the server is missing from the output', () => {
    expect(checkServerGraph(parseCargoTree('protocol v0.1.0\n'))).toEqual([
      'server: not in the cargo tree output',
    ]);
  });
});

describe('checkWasmSources (RF19)', () => {
  const exported = '#[wasm_bindgen]\npub fn simulate() {}\n';
  const check = (text) => checkWasmSources([{ file: 'lib.rs', text }]);
  const RELAXED = /^lib\.rs: mentions unsafe_code \d+ time\(s\) outside a deny\/forbid list/;
  it('passes with the real sources', () => {
    const text = readFileSync(
      new URL('../../crates/engine-wasm/src/lib.rs', import.meta.url),
      'utf8',
    );
    expect(check(text)).toEqual([]);
    expect(text).not.toMatch(/unsafe_code/);
  });
  it('fails with the token unsafe (code or comment)', () => {
    expect(check(`${exported}unsafe { x() }\n`)).toEqual([
      'lib.rs: contains the token `unsafe` 1 time(s) (RF19)',
    ]);
    expect(check('// this is unsafe\n')).toHaveLength(1);
  });
  it.each([
    ['a direct allow', `#[allow(unsafe_code)]\n${exported}`],
    ['an allow among other lints', `#[allow(dead_code, unsafe_code)]\n${exported}`],
    ['a nested list', `#[allow(unsafe_code, clippy::all)]\n${exported}`],
    ['a crate-level allow', `#![allow(unsafe_code)]\n${exported}`],
    ['an expect', `#[expect(unsafe_code)]\n${exported}`],
    ['a warn', `#[warn(unsafe_code)]\n${exported}`],
    ['cfg_attr(all(), allow(unsafe_code))', `#[cfg_attr(all(), allow(unsafe_code))]\n${exported}`],
    [
      'a macro that generates #[allow($l)]',
      'macro_rules! relax { ($l:ident) => { #[allow($l)] #[wasm_bindgen] pub fn third() {} } }\nrelax!(unsafe_code);\n',
    ],
    [
      'a macro body that names it',
      'macro_rules! m { () => { #[allow(unsafe_code)] fn f() {} } }\n',
    ],
    ['a multi-line attribute', `#[allow(\n    dead_code,\n    unsafe_code,\n)]\n${exported}`],
    [
      'code after a string with //',
      `const U: &str = "http://x"; #[allow(unsafe_code)]\n${exported}`,
    ],
  ])('fails with %s', (_, text) => {
    expect(check(text)).toEqual([expect.stringMatching(RELAXED)]);
  });
  it('fails with #[path = ...] mod and with include!', () => {
    expect(check('#[path = "../shim.rs"]\nmod shim;\n')).toEqual([
      'lib.rs: uses a #[path] attribute (RF19)',
    ]);
    expect(check('#[cfg_attr(all(), path = "../shim.rs")]\nmod shim;\n')).toEqual([
      'lib.rs: uses a #[path] attribute (RF19)',
    ]);
    expect(check('include!("../shim.rs");\n')).toEqual(['lib.rs: uses include! (RF19)']);
    expect(check('include ! ("../shim.rs");\n')).toEqual(['lib.rs: uses include! (RF19)']);
  });
  it('allows include_str!, include_bytes! and deny/forbid of unsafe_code', () => {
    const text = `#![deny(unsafe_code)]\n#![forbid(unsafe_code, dead_code)]\nconst D: &[u8] = include_bytes!("x");\nconst S: &str = include_str!("y");\n${exported}`;
    expect(check(text)).toEqual([]);
  });
  it('ignores unsafe_code in line and block comments, but not after them', () => {
    expect(
      check(
        `// #[allow(unsafe_code)]\n/* #[allow(unsafe_code)] /* nested */ unsafe_code */\n${exported}`,
      ),
    ).toEqual([]);
    expect(check(`/* comment */ #[allow(unsafe_code)]\n${exported}`)).toEqual([
      expect.stringMatching(RELAXED),
    ]);
    expect(
      check(`/* a /* nested */ still a comment */ #[allow(unsafe_code)]\n${exported}`),
    ).toEqual([expect.stringMatching(RELAXED)]);
  });
  it('checks every file', () => {
    expect(
      checkWasmSources([
        { file: 'lib.rs', text: exported },
        { file: 'other.rs', text: '#[allow(unsafe_code)]\nfn f() {}\n' },
      ]),
    ).toEqual([expect.stringMatching(/^other\.rs: mentions unsafe_code/)]);
  });
});

describe('stripComments', () => {
  it('blanks comments, keeps newlines, strings and char literals', () => {
    expect(stripComments('a // b\nc /* d\ne */ f')).toBe('a     \nc     \n     f');
    expect(stripComments('"// no" \'/\' r#"/* no */"# x')).toBe('"// no" \'/\' r#"/* no */"# x');
    expect(stripComments("fn f<'a>(x: &'a str) {} // c")).toBe("fn f<'a>(x: &'a str) {}     ");
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
  const workspace = '[workspace.lints.rust]\nunsafe_code = "forbid"\n';
  const inherits = '[package]\nname = "engine-wasm"\n\n[lints]\nworkspace = true\n';
  it('passes with the real files', () => {
    expect(checkWasmLints(WORKSPACE_TOML, WASM_TOML)).toEqual([]);
  });
  it('passes when the crate inherits the workspace lints', () => {
    expect(checkWasmLints(workspace, inherits)).toEqual([]);
  });
  it('fails without [lints] workspace = true', () => {
    expect(checkWasmLints(workspace, '[package]\nname = "engine-wasm"\n')).toEqual([
      'crates/engine-wasm/Cargo.toml: missing [lints] workspace = true (RF19)',
    ]);
    expect(checkWasmLints(workspace, '[lints]\nworkspace = false\n')).toHaveLength(2);
  });
  it('fails with a [lints.*] table of its own (the old deny design)', () => {
    const own = '[lints.rust]\nunsafe_code = "deny"\n\n[lints.clippy]\nunwrap_used = "deny"\n';
    expect(checkWasmLints(workspace, own)).toEqual([
      'crates/engine-wasm/Cargo.toml: own [lints.rust] table (RF19: inherit the workspace lints)',
      'crates/engine-wasm/Cargo.toml: own [lints.clippy] table (RF19: inherit the workspace lints)',
      'crates/engine-wasm/Cargo.toml: missing [lints] workspace = true (RF19)',
    ]);
    expect(
      checkWasmLints(workspace, `${inherits}\n[lints.rust]\nunsafe_code = "allow"\n`),
    ).toHaveLength(1);
  });
  it('fails with extra keys in [lints] or dotted lints keys', () => {
    expect(checkWasmLints(workspace, `${inherits}rust = { unsafe_code = "allow" }\n`)).toHaveLength(
      1,
    );
    expect(checkWasmLints(workspace, `${inherits}[package.metadata]\nx = 1\n`)).toEqual([]);
    expect(
      checkWasmLints(
        workspace,
        '[package]\nlints.rust.unsafe_code = "allow"\n[lints]\nworkspace = true\n',
      ),
    ).toHaveLength(1);
  });
  it('fails if the workspace does not forbid unsafe_code', () => {
    expect(checkWasmLints('[workspace.lints.rust]\nunsafe_code = "deny"\n', inherits)).toEqual([
      'Cargo.toml: [workspace.lints.rust] must have unsafe_code = "forbid"',
    ]);
  });
});
