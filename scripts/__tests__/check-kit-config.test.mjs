import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { checkCspSource, checkProvidersSource, checkKitConfig } from '../check-kit-config.mjs';

const FIXTURES = fileURLToPath(new URL('../__fixtures__/check-kit-config/', import.meta.url));
const run = (name) => checkKitConfig(path.join(FIXTURES, name));

describe('check-kit-config (RF-A10, plan-a decision 17)', () => {
  it('passes on a valid config, even with the names in a comment', () => {
    expect(run('valid')).toEqual([]);
  });
  it.each([
    ['trusted-origins', /vite\.config\.ts:\d+: `trustedOrigins`/],
    ['check-origin', /vite\.config\.ts:\d+: `checkOrigin`/],
    ['paths-origin', /vite\.config\.ts:\d+: `origin` inside `paths`/],
    ['paths-origin-dotted', /vite\.config\.ts:\d+: `paths\.origin`/],
    ['svelte-config', /svelte\.config\.js:1: /],
  ])('fails on %s', (name, pattern) => {
    const problems = run(name);
    expect(
      problems.some((p) => pattern.test(p)),
      problems.join('\n'),
    ).toBe(true);
  });
  it('passes on the real apps/web', () => {
    expect(checkKitConfig(fileURLToPath(new URL('../../apps/web', import.meta.url)))).toEqual([]);
  });
});

const GOOGLE = 'https://accounts.google.com/o/oauth2/v2/auth';
const DISCORD = 'https://discord.com/oauth2/authorize';

const BASE = {
  'default-src': ['self'],
  'script-src': ['self'],
  'style-src': ['self', 'unsafe-inline'],
  'img-src': ['self', 'data:', 'blob:'],
  'font-src': ['self'],
  'connect-src': ['self'],
  'object-src': ['none'],
  'base-uri': ['self'],
  'form-action': ['self'],
  'frame-ancestors': ['none'],
};

/** A vite.config.ts whose `csp` is `options` with `directives` = BASE changed by `edit`. */
function config(edit = (d) => d, options = (o) => o) {
  const directives = edit(structuredClone(BASE));
  const csp = options({ mode: 'auto', directives });
  const body = JSON.stringify(csp, null, 2).replaceAll('"', "'");
  return `import { sveltekit } from '@sveltejs/kit/vite';\nexport default {\n  plugins: [sveltekit({ csp: ${body} })],\n};\n`;
}
const add =
  (directive, ...sources) =>
  (d) => ({
    ...d,
    [directive]: [...(d[directive] ?? []), ...sources],
  });
const csp = (source) => checkCspSource(source, 'vite.config.ts');

describe('check-kit-config: csp is the exact B1 allowlist (ADR-0012 consequence 6)', () => {
  it('passes on B1', () => {
    expect(csp(config())).toEqual([]);
  });
  it.each([
    ["'wasm-unsafe-eval' in script-src (B4.1)", add('script-src', 'wasm-unsafe-eval')],
    ["worker-src 'self' (B4.3)", add('worker-src', 'self')],
    [
      'both B4 extensions',
      (d) => add('worker-src', 'self')(add('script-src', 'wasm-unsafe-eval')(d)),
    ],
    // B4.6 (2026-10-30 amendment): the authorize endpoints of the two providers, with their path.
    ['the two B4.6 sources in form-action', add('form-action', GOOGLE, DISCORD)],
    ['one B4.6 source in form-action', add('form-action', DISCORD)],
  ])('allows %s', (_, edit) => {
    expect(csp(config(edit))).toEqual([]);
  });

  const forbidden = [
    // B3: script-src
    ['script-src', 'unsafe-eval'],
    ['script-src', 'unsafe-inline'],
    ['script-src', 'unsafe-hashes'],
    ['script-src', 'strict-dynamic'],
    ['script-src', 'data:'],
    ['script-src', 'blob:'],
    // B3: wildcards, bare schemes and external origins in any directive
    ['default-src', '*'],
    ['img-src', '*.example.com'],
    ['connect-src', 'https:'],
    ['connect-src', 'http:'],
    ['connect-src', 'wss:'],
    ['connect-src', 'ws:'],
    ['script-src', 'https://cdn.example.com'],
    ['font-src', 'https://fonts.example.com'],
    ['connect-src', 'https://analytics.example.com'],
    // B3: data: and blob: outside img-src
    ['default-src', 'data:'],
    ['connect-src', 'data:'],
    ['worker-src', 'data:'],
    ['default-src', 'blob:'],
    ['connect-src', 'blob:'],
    ['worker-src', 'blob:'],
    ['style-src', 'blob:'],
    // B3: relaxing object-src, base-uri, form-action or frame-ancestors
    ['object-src', 'self'],
    ['base-uri', 'none'],
    ['form-action', 'https://evil.example'],
    // B4.6: exactly the two sources, with their path, and only in form-action.
    ['form-action', 'https://accounts.google.com'],
    ['form-action', 'https://discord.com'],
    ['form-action', 'https://discord.com/'],
    ['form-action', 'https://discord.com/api/webhooks/x'],
    ['form-action', 'https://discord.com/oauth2/authorize/'],
    ['form-action', 'https://accounts.google.com/o/oauth2/auth'],
    ['form-action', 'https://evil.example/oauth2/authorize'],
    ['form-action', 'https://discord.com:8443/oauth2/authorize'],
    ['form-action', 'http://discord.com/oauth2/authorize'],
    ['form-action', 'https:'],
    ['form-action', '*'],
    ['form-action', 'https://*.discord.com/oauth2/authorize'],
    ['connect-src', 'https://discord.com/oauth2/authorize'],
    ['default-src', 'https://accounts.google.com/o/oauth2/v2/auth'],
    ['script-src', 'https://accounts.google.com/o/oauth2/v2/auth'],
    ['frame-ancestors', 'self'],
    // B3: 'wasm-unsafe-eval' outside script-src
    ['default-src', 'wasm-unsafe-eval'],
    ['worker-src', 'wasm-unsafe-eval'],
    // B3: nonces and hashes written in the config
    ['script-src', 'nonce-dev'],
    ['style-src', 'nonce-dev'],
    ['script-src', 'sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA='],
    ['script-src', 'sha384-AAAA'],
    ['script-src', 'sha512-AAAA'],
    // B3: 'unsafe-allow-redirects' and 'inline-speculation-rules'
    ['navigate-to', 'unsafe-allow-redirects'],
    ['script-src', 'unsafe-allow-redirects'],
    ['script-src', 'inline-speculation-rules'],
    // B3: other schemes in img-src
    ['img-src', 'filesystem:'],
    ['img-src', 'mediastream:'],
  ];
  it.each(forbidden)('fails on %s %s', (directive, source) => {
    const problems = csp(config(add(directive, source)));
    expect(problems.length, 'expected a problem').toBeGreaterThan(0);
    expect(problems.join('\n')).toMatch(/csp \(ADR-0012 B1\/B4\)/);
  });

  it.each(Object.keys(BASE))('fails when %s is missing', (name) => {
    const problems = csp(
      config((d) => {
        delete d[name];
        return d;
      }),
    );
    expect(problems).toContainEqual(expect.stringContaining(`directive \`${name}\` is missing`));
  });
  it.each(['object-src', 'base-uri', 'form-action', 'frame-ancestors', 'script-src'])(
    'fails when %s loses its B1 source',
    (name) => {
      const problems = csp(config((d) => ({ ...d, [name]: [] })));
      expect(problems).toContainEqual(expect.stringContaining(`\`${name}\` must contain`));
    },
  );
  it.each(['report-uri', 'report-to', 'require-trusted-types-for', 'upgrade-insecure-requests'])(
    'fails on the extra directive %s',
    (name) => {
      const problems = csp(config(add(name, 'self')));
      expect(problems).toContainEqual(
        expect.stringContaining(`directive \`${name}\` is not allowed`),
      );
    },
  );
  it('fails on worker-src without self', () => {
    expect(csp(config((d) => ({ ...d, 'worker-src': [] })))).toContainEqual(
      expect.stringContaining("`worker-src` must contain 'self'"),
    );
  });
  it('fails on a repeated source', () => {
    expect(csp(config(add('script-src', 'self')))).toContainEqual(
      expect.stringContaining("`script-src` repeats 'self'"),
    );
  });
  it.each(['hash', 'nonce', 'Auto'])("fails on mode '%s'", (mode) => {
    expect(csp(config(undefined, (o) => ({ ...o, mode })))).toContainEqual(
      expect.stringContaining("`mode` must be 'auto'"),
    );
  });
  it('fails without mode', () => {
    expect(csp(config(undefined, ({ directives }) => ({ directives })))).toContainEqual(
      expect.stringContaining("`mode` must be 'auto'"),
    );
  });
  it.each(['reportOnly', 'nonce'])('fails on the extra option %s', (name) => {
    const problems = csp(config(undefined, (o) => ({ ...o, [name]: { 'script-src': ['self'] } })));
    expect(problems).toContainEqual(expect.stringContaining(`option \`${name}\` is not allowed`));
  });
  it('fails without the csp option', () => {
    expect(csp(config().replace(/csp: \{[\s\S]*\}\s*\}\)\]/, 'adapter })]'))).toEqual([
      'vite.config.ts:3: csp (ADR-0012 B1/B4): the `csp` option is missing',
    ]);
  });
  it('ignores csp in comments, strings and unrelated variables', () => {
    const source = `// csp: { mode: 'hash' }\nconst csp = { mode: 'hash' };\nconst s = "sveltekit({ csp: {} })";\n${config()}`;
    expect(csp(source)).toEqual([]);
  });
  it('reads escapes as the value Kit receives', () => {
    expect(csp(config().replace("'self'", "'\\x73elf'"))).toEqual([]);
    expect(
      csp(config(add('connect-src', 'https:')).replace("'https:'", "'\\x68ttps:'")),
    ).toContainEqual(expect.stringContaining("must not contain 'https:'"));
  });
  it('fails on invalid TypeScript', () => {
    expect(csp(`${config()}\nconst = ;\n`)).toContainEqual(
      expect.stringContaining('not valid TypeScript'),
    );
  });

  // security-reviewer and architect on #50: the literal must be the policy Kit receives.
  const RELAXED = "{ mode: 'auto', directives: { 'script-src': ['self', 'unsafe-inline'] } }";
  const IMPORT = "import { sveltekit } from '@sveltejs/kit/vite';";
  it.each([
    // Values that are not literals
    ['a spread in csp', config().replace("'mode'", "...extra, 'mode'"), /keys must be plain/],
    ['an identifier value', config().replace("'auto'", 'MODE'), /`MODE` is not allowed/],
    ['a template literal', config().replace("'auto'", '`auto`'), /``auto`` is not allowed/],
    [
      'a call in a directive',
      config().replace("'default-src': [", "'default-src': sources() || ["),
      /`sources\(\) \|\| \[/,
    ],
    [
      'a value chosen by the environment',
      config().replace('csp: {', `csp: process.env.CI ? ${RELAXED} : {`),
      /must be written `csp: \{ … \}`/,
    ],
    // The options object
    [
      'a computed key after csp',
      config().replace('})],', `, ['c' + 'sp']: ${RELAXED} })],`),
      /keys must be plain/,
    ],
    ['a spread after csp', config().replace('})],', ', ...kitExtra })],'), /keys must be plain/],
    [
      'a spread before csp',
      config().replace('sveltekit({ csp', 'sveltekit({ ...kitExtra, csp'),
      /keys must be plain/,
    ],
    [
      'a shorthand csp',
      config().replace('sveltekit({ csp: ', 'sveltekit({ csp, x: '),
      /must be written/,
    ],
    [
      'a quoted csp key repeated',
      config().replace('})],', `, 'csp': ${RELAXED} })],`),
      /repeats `csp`/,
    ],
    [
      'csp nested in another key',
      config().replace('sveltekit({ csp', 'sveltekit({ kit: { csp').replace('})],', '} })],'),
      /the `csp` option is missing/,
    ],
    [
      'Object.assign around the options',
      config()
        .replace('sveltekit({ csp', 'sveltekit(Object.assign({ csp')
        .replace('})],', '}, extra))],'),
      /must receive one object literal/,
    ],
    ['a second argument', config().replace('})],', '}, extra)],'), /one object literal/],
    // The call and the import
    ['a second sveltekit call', `${config()}sveltekit({});\n`, /called exactly once/],
    [
      'a decoy call in an unused function',
      `${config()}function decoy() { return sveltekit({ csp: {} }); }\n`,
      /called exactly once/,
    ],
    [
      'an aliased import',
      config()
        .replace(IMPORT, "import { sveltekit as kit } from '@sveltejs/kit/vite';")
        .replace('sveltekit({ csp: ', `kit({ ['c' + 'sp']: ${RELAXED}, x: `),
      /imported once, unaliased/,
    ],
    [
      'a local sveltekit bound to an alias, with a decoy call',
      config()
        .replace(
          IMPORT,
          "import { sveltekit as kit } from '@sveltejs/kit/vite';\nconst sveltekit = kit;",
        )
        .replace('[sveltekit({', `[kit({ ['c' + 'sp']: ${RELAXED} }), (() => sveltekit({`)
        .replace('})],', '}))],'),
      /imported once, unaliased/,
    ],
    [
      'a namespace import',
      `import * as kitVite from '@sveltejs/kit/vite';\n${config()}`,
      /imported once, unaliased/,
    ],
    [
      'a second import of the module',
      `${config()}const kitVite = await import('@sveltejs/kit/vite');\n`,
      /imported once, unaliased/,
    ],
    ['no import', config().replace(IMPORT, ''), /imported once, unaliased/],
    [
      'sveltekit outside plugins',
      config().replace('plugins: [sveltekit(', 'other: [sveltekit('),
      /element of the exported config `plugins/,
    ],
    // security-reviewer round 3: a decoy config next to a plugin from elsewhere
    [
      'a decoy config that is not exported',
      config().replace('export default {', 'const unused = {') +
        'export default { plugins: [] };\n',
      /element of the exported config/,
    ],
    [
      'a dynamic import',
      `${config()}const kv = await import('@sveltejs/' + 'kit/vite');\n`,
      /`import\(\)` is not allowed/,
    ],
    [
      'defineConfig with a second argument',
      config()
        .replace('export default {', 'export default defineConfig({')
        .replace('};\n', '}, extra);\n'),
      /element of the exported config/,
    ],
    // security-reviewer round 2: text tricks that fooled the lexer
    [
      'a regex read as a division',
      `void /'/;\n` + config().replace('})],', `, ['c' + 'sp']: ${RELAXED} })],`) + "void /'/;\n",
      /keys must be plain/,
    ],
    [
      'code inside a template expression',
      config().replace('sveltekit({ csp: ', `sveltekit({ x: \`\${0}\`, ['csp']: ${RELAXED}, y: `),
      /keys must be plain/,
    ],
  ])('fails on %s', (_, source, pattern) => {
    const problems = csp(source);
    expect(problems.join('\n')).toMatch(pattern);
  });
  it('accepts the exported config wrapped in defineConfig', () => {
    const source = config()
      .replace('export default {', 'export default defineConfig({')
      .replace('};\n', '});\n');
    expect(csp(source)).toEqual([]);
  });
  it('accepts a quoted csp key, shorthand keys and a regex literal elsewhere', () => {
    const source = `const adapter = {};\nconst re = /'"\`/;\n${config().replace('sveltekit({ csp', "sveltekit({ adapter, 'csp'")}`;
    expect(csp(source)).toEqual([]);
  });
  it('fails cleanly on a prototype name as a directive', () => {
    expect(csp(config((d) => ({ ...d, constructor: ['self'] })))).toContainEqual(
      expect.stringContaining('directive `constructor` is not allowed'),
    );
  });
  it('reports the line of the offending source', () => {
    const source = config(add('connect-src', 'https:'));
    const line = source.split('\n').findIndex((l) => l.includes("'https:'")) + 1;
    expect(csp(source)).toEqual([
      `vite.config.ts:${line}: csp (ADR-0012 B1/B4): \`connect-src\` must not contain 'https:'`,
    ]);
  });
});

// B4.6 is tied to providers.rs (security-reviewer, recommendation of the amendment): the two
// constants of the server are exactly the two sources of the allowlist.
describe('check-kit-config: B4.6 matches GOOGLE_AUTHORIZE and DISCORD_AUTHORIZE', () => {
  const RS = `pub const GOOGLE_AUTHORIZE: &str = "${GOOGLE}";
pub const GOOGLE_TOKEN: &str = "https://oauth2.googleapis.com/token";
pub const DISCORD_AUTHORIZE: &str = "${DISCORD}";

#[cfg(test)]
mod tests {
    const GOOGLE_AUTHORIZE: &str = "https://decoy.example/x";
}
`;
  const check = (text) => checkProvidersSource(text, 'providers.rs');

  it('passes on the real providers.rs', () => {
    const real = readFileSync(
      new URL('../../apps/server/src/auth/providers.rs', import.meta.url),
      'utf8',
    );
    expect(check(real)).toEqual([]);
  });

  it('passes on matching constants, ignoring the test module', () => {
    expect(check(RS)).toEqual([]);
  });

  it.each([
    ['a drifted Google URL', RS.replace(GOOGLE, 'https://accounts.google.com/o/oauth2/auth')],
    ['a drifted Discord URL', RS.replace(DISCORD, 'https://discord.com/api/oauth2/authorize')],
    ['a missing constant', RS.replace(/pub const DISCORD_AUTHORIZE[^\n]*\n/, '')],
    [
      'a repeated constant',
      RS.replace(
        'pub const GOOGLE_TOKEN',
        `pub const GOOGLE_AUTHORIZE: &str = "${GOOGLE}";\npub const GOOGLE_TOKEN`,
      ),
    ],
  ])('fails on %s', (_name, text) => {
    expect(check(text).length).toBeGreaterThan(0);
    expect(check(text).join('\n')).toMatch(/B4\.6/);
  });
});
