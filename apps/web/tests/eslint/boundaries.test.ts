import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { ESLint } from 'eslint';
import { describe, expect, it } from 'vitest';
import { readdirSync } from 'node:fs';
import { UI_MAX_DEPTH, createConfig } from '../../eslint.config.js';

// ADR-0009 test 2 (with the Kit 3 `#lib` amendment), RF-A14 and plan-a decisions 2, 18 and 20.
const WEB = fileURLToPath(new URL('../..', import.meta.url));

function linter(relativePatterns = true): ESLint {
  return new ESLint({
    cwd: WEB,
    overrideConfigFile: true,
    overrideConfig: createConfig({ relativePatterns }),
  });
}
const full = linter(true);
const aliasOnly = linter(false);

async function rules(file: string, code: string, eslint = full): Promise<string[]> {
  const [result] = await eslint.lintText(code, { filePath: path.join(WEB, file) });
  const fatal = result?.messages.find((m) => m.fatal);
  if (fatal) throw new Error(`parse error in ${file}: ${fatal.message}`);
  return (result?.messages ?? []).map((m) => m.ruleId ?? 'unknown');
}

const svelte = (script: string, markup = '<div></div>') =>
  `<script lang="ts">\n${script}\n</script>\n\n${markup}\n`;

const IMPORTS = '@typescript-eslint/no-restricted-imports';
const SYNTAX = 'no-restricted-syntax';
const DYNAMIC = 'torneo/no-restricted-dynamic-imports';

describe('import boundaries that fail', () => {
  it.each([
    [
      'renderer value outside FightStage',
      'src/lib/components/fight/FightHud.svelte',
      svelte("  import { createRenderer } from '@torneo/renderer';\n  createRenderer;"),
    ],
    [
      'renderer value by relative path',
      'src/routes/+page.ts',
      "import { createRenderer } from '../../../../packages/renderer/src/index.js';\nexport const x = createRenderer;\n",
    ],
    [
      'feature value in a domain component (#lib)',
      'src/lib/components/fight/Hud.svelte',
      svelte("  import { health } from '#lib/features/system/health.js';\n  health;"),
    ],
    [
      'feature value in a domain component (relative)',
      'src/lib/components/fight/Hud.svelte',
      svelte("  import { health } from '../../features/x.js';\n  health;"),
    ],
    [
      'feature type in ui (#lib)',
      'src/lib/components/ui/button/button.svelte',
      svelte(
        "  import type { Health } from '#lib/features/system/health.js';\n  let h: Health | undefined;\n  h;",
      ),
    ],
    [
      'feature type in ui (relative)',
      'src/lib/components/ui/button/helpers.ts',
      "import type { Health } from '../../../features/system/health.js';\nexport type H = Health;\n",
    ],
    [
      'domain component in ui',
      'src/lib/components/ui/card/card.svelte',
      svelte("  import Hud from '#lib/components/fight/Hud.svelte';\n  Hud;"),
    ],
    [
      'domain component in ui (relative)',
      'src/lib/components/ui/card/card.svelte',
      svelte("  import Hud from '../../fight/Hud.svelte';\n  Hud;"),
    ],
    // TA-10: a domain folder that is not in any list is still off limits for ui/.
    [
      'unlisted domain component in ui (#lib)',
      'src/lib/components/ui/card/card.svelte',
      svelte("  import Chat from '#lib/components/chat/Chat.svelte';\n  Chat;"),
    ],
    [
      'unlisted domain component in ui (relative)',
      'src/lib/components/ui/card/card.svelte',
      svelte("  import Chat from '../../chat/Chat.svelte';\n  Chat;"),
    ],
    [
      'unlisted domain component in ui (relative, from ui root)',
      'src/lib/components/ui/x.ts',
      "import Chat from '../chat/Chat.svelte';\nexport const x = Chat;\n",
    ],
    [
      'Paraglide messages in ui (#lib)',
      'src/lib/components/ui/dialog/dialog-content.svelte',
      svelte("  import { m } from '#lib/paraglide/messages.js';\n  m;"),
    ],
    [
      'Paraglide messages in ui (relative)',
      'src/lib/components/ui/dialog/dialog-content.svelte',
      svelte("  import { m } from '../../../paraglide/messages.js';\n  m;"),
    ],
    [
      'setLocale (#lib)',
      'src/lib/components/layout/LocaleSwitcher.svelte',
      svelte("  import { setLocale } from '#lib/paraglide/runtime.js';\n  setLocale;"),
    ],
    [
      'setLocale (relative)',
      'src/routes/+page.ts',
      "import { setLocale } from '../lib/paraglide/runtime.js';\nexport const x = setLocale;\n",
    ],
    [
      '$lib/utils',
      'src/routes/+page.ts',
      "import { cn } from '$lib/utils';\nexport const x = cn;\n",
    ],
    ['$lib', 'src/routes/+page.ts', "import lib from '$lib';\nexport const x = lib;\n"],
    [
      '$env/dynamic/private',
      'src/hooks.server.ts',
      "import { env } from '$env/dynamic/private';\nexport const x = env;\n",
    ],
    [
      '$app/environment',
      'src/routes/+layout.ts',
      "import { browser } from '$app/environment';\nexport const x = browser;\n",
    ],
  ])('%s', async (_name, file, code) => {
    expect(await rules(file, code)).toContain(IMPORTS);
  });
});

// 008 plan-d decision 8: API mutations only from the actions of a `+page.server.ts`.
describe('api/mutations only from +page.server.ts', () => {
  const IMPORT =
    "import { postUnlink } from '#lib/server/api/mutations.js';\nexport const x = postUnlink;\n";
  const RELATIVE =
    "import { postUnlink } from '../lib/server/api/mutations.js';\nexport const x = postUnlink;\n";
  it.each([
    ['src/hooks.server.ts', IMPORT],
    ['src/routes/+layout.server.ts', RELATIVE],
    ['src/routes/account/+layout.server.ts', IMPORT],
    ['src/routes/account/+page.ts', IMPORT],
    ['src/routes/account/+layout.ts', IMPORT],
    ['src/routes/healthz/+server.ts', IMPORT],
    ['src/lib/server/hooks/session.ts', IMPORT],
    ['src/lib/features/account/x.ts', IMPORT],
    [
      'src/lib/components/account/ProviderList.svelte',
      svelte("  import { postUnlink } from '#lib/server/api/mutations.js';\n  postUnlink;"),
    ],
  ])('%s fails', async (file, code) => {
    expect(await rules(file, code)).toContain(IMPORTS);
  });

  it('a dynamic import() fails too', async () => {
    expect(
      await rules(
        'src/routes/+layout.server.ts',
        "export const x = () => import('#lib/server/api/mutations.js');\n",
      ),
    ).toContain(DYNAMIC);
  });

  it.each([
    ['src/routes/account/+page.server.ts', IMPORT],
    ['src/routes/onboarding/+page.server.ts', IMPORT],
    ['src/routes/preferences/+page.server.ts', RELATIVE.replace('../lib', '../../lib')],
  ])('%s passes', async (file, code) => {
    expect(await rules(file, code)).toEqual([]);
  });
});

// 008 plan-d decision 13 (change 6 of architect): postgres.js is a devDependency for the e2e
// database helpers only; it never reaches the app bundle.
describe('postgres only from e2e/', () => {
  const PG = "import postgres from 'postgres';\nexport const sql = postgres;\n";
  it.each([
    ['src/lib/server/db.ts', PG],
    ['src/hooks.server.ts', PG],
    ['src/routes/account/+page.server.ts', PG],
    [
      'src/lib/components/account/X.svelte',
      svelte("  import postgres from 'postgres';\n  postgres;"),
    ],
    ['tests/some.test.ts', PG],
  ])('%s fails', async (file, code) => {
    expect(await rules(file, code)).toContain(IMPORTS);
  });

  it('import() fails too', async () => {
    expect(
      await rules('src/lib/server/x.ts', "export const x = () => import('postgres');\n"),
    ).toContain(DYNAMIC);
  });

  it.each(['e2e/support/db.mjs', 'tests/e2e-db.test.ts'])('%s passes', async (file) => {
    expect(await rules(file, PG)).toEqual([]);
  });
});

describe('each #lib pattern works alone (relative twins disabled)', () => {
  it.each([
    [
      'src/lib/components/fight/Hud.svelte',
      svelte("  import { health } from '#lib/features/system/health.js';\n  health;"),
    ],
    [
      'src/lib/components/ui/button/button.svelte',
      svelte(
        "  import type { Health } from '#lib/features/system/health.js';\n  let h: Health | undefined;\n  h;",
      ),
    ],
    [
      'src/lib/components/ui/card/card.svelte',
      svelte("  import Hud from '#lib/components/fight/Hud.svelte';\n  Hud;"),
    ],
    [
      'src/lib/components/ui/card/card.svelte',
      svelte("  import Chat from '#lib/components/chat/Chat.svelte';\n  Chat;"),
    ],
    [
      'src/lib/components/ui/dialog/x.svelte',
      svelte("  import { m } from '#lib/paraglide/messages.js';\n  m;"),
    ],
    [
      'src/routes/+page.ts',
      "import { setLocale } from '#lib/paraglide/runtime.js';\nexport const x = setLocale;\n",
    ],
    [
      'src/routes/+page.ts',
      "import { createRenderer } from '@torneo/renderer';\nexport const x = createRenderer;\n",
    ],
  ])('%s', async (file, code) => {
    expect(await rules(file, code, aliasOnly)).toContain(IMPORTS);
  });
});

describe('other rules that fail', () => {
  it('fetch in a component', async () => {
    expect(
      await rules('src/lib/components/fight/Hud.svelte', svelte("  fetch('/api/health');")),
    ).toContain('no-restricted-globals');
  });

  it('console.log outside log.ts', async () => {
    expect(await rules('src/lib/i18n/format.ts', "console.log('x');\n")).toContain('no-console');
  });

  it.each([
    ['event.url.origin', 'export const f = (event: { url: URL }) => event.url.origin;\n'],
    [
      'url.host after destructuring',
      'export const f = (event: { url: URL }) => {\n  const { url } = event;\n  return url.host;\n};\n',
    ],
    ['request.url', 'export const f = (request: Request) => request.url;\n'],
    ['event.request.url', 'export const f = (event: { request: Request }) => event.request.url;\n'],
    [
      'new URL with a base',
      "export const f = (event: { url: URL }) => new URL('/x', event.url);\n",
    ],
    ['event.url.toString()', 'export const f = (event: { url: URL }) => event.url.toString();\n'],
    ['String(event.url)', 'export const f = (event: { url: URL }) => String(event.url);\n'],
    ['template with event.url', 'export const f = (event: { url: URL }) => `${event.url}`;\n'],
    // TA-10: destructuring.
    [
      'const { origin } = event.url',
      'export const f = (event: { url: URL }) => {\n  const { origin } = event.url;\n  return origin;\n};\n',
    ],
    [
      'const { href: h } = url',
      'export const f = (url: URL) => {\n  const { href: h } = url;\n  return h;\n};\n',
    ],
    [
      '({ url: { host } }) parameter',
      'export const f = ({ url: { host } }: { url: URL }) => host;\n',
    ],
    [
      'const { url: { origin } } = event',
      'export const f = (event: { url: URL }) => {\n  const {\n    url: { origin },\n  } = event;\n  return origin;\n};\n',
    ],
    [
      '({ origin } = event.url) assignment',
      "export const f = (event: { url: URL }) => {\n  let origin = '';\n  ({ origin } = event.url);\n  return origin;\n};\n",
    ],
    // TA-10: `new URL(<x>.url)` copies the client-controlled `Host`.
    [
      'new URL(event.url).origin',
      'export const f = (event: { url: URL }) => new URL(event.url).origin;\n',
    ],
    ['new URL(url).href', 'export const f = (url: URL) => new URL(url).href;\n'],
    // TA-10: URLs built from the `Host` or `X-Forwarded-*` headers.
    [
      "request.headers.get('host')",
      "export const f = (request: Request) => `https://${request.headers.get('host')}/x`;\n",
    ],
    [
      "event.request.headers.get('X-Forwarded-Host')",
      "export const f = (event: { request: Request }) => event.request.headers.get('X-Forwarded-Host');\n",
    ],
    [
      "headers.get('x-forwarded-proto')",
      "export const f = (headers: Headers) => headers.get('x-forwarded-proto');\n",
    ],
    [
      "headers.get('forwarded')",
      "export const f = (headers: Headers) => headers.get('Forwarded');\n",
    ],
    [
      'headers.host (node headers)',
      'export const f = (req: { headers: Record<string, string> }) => req.headers.host;\n',
    ],
    [
      "headers['x-forwarded-host']",
      "export const f = (req: { headers: Record<string, string> }) => req.headers['x-forwarded-host'];\n",
    ],
    // PR #2 review (security-reviewer).
    [
      'hostname, port and protocol',
      "export const f = (event: { url: URL }) =>\n  event.url.protocol + '//' + event.url.hostname + ':' + event.url.port;\n",
    ],
    [
      'const { hostname } = event.url',
      'export const f = (event: { url: URL }) => {\n  const { hostname } = event.url;\n  return hostname;\n};\n',
    ],
    [
      "URL.parse('/x', event.url)",
      "export const f = (event: { url: URL }) => URL.parse('/x', event.url);\n",
    ],
    ['URL.parse(event.url)', 'export const f = (event: { url: URL }) => URL.parse(event.url);\n'],
    ["event.url['origin']", "export const f = (event: { url: URL }) => event.url['origin'];\n"],
    ['event.url.toJSON()', 'export const f = (event: { url: URL }) => event.url.toJSON();\n'],
    ["'' + event.url", "export const f = (event: { url: URL }) => '' + event.url;\n"],
    [
      'new Request(event.url)',
      'export const f = (event: { url: URL }) => new Request(event.url);\n',
    ],
    ['headers.get(`host`)', 'export const f = (headers: Headers) => headers.get(`host`);\n'],
    [
      'request.headers.get(`x-forwarded-${p}`)',
      'export const f = (request: Request, p: string) => request.headers.get(`x-forwarded-${p}`);\n',
    ],
    [
      'Object.fromEntries(request.headers).host',
      'export const f = (request: Request) => Object.fromEntries(request.headers).host;\n',
    ],
  ])('absolute URL: %s', async (_name, code) => {
    expect(await rules('src/lib/server/hooks/x.ts', code)).toContain(SYNTAX);
  });
});

describe('dynamic imports of the renderer (002 plan-a2 decision 11, TA-11)', () => {
  it.each([
    [
      'import() in another component',
      'src/lib/components/fight/ArenaControls.svelte',
      svelte("  void import('@torneo/renderer');"),
    ],
    [
      'import() of a subpath in a route',
      'src/routes/dev/arena/+page.svelte',
      svelte("  void import('@torneo/renderer/fixtures/spike.timeline.json');"),
    ],
    [
      'import() by relative path in a .ts',
      'src/lib/features/arena/x.ts',
      "export const load = () => import('../../../../../packages/renderer/src/index.ts');\n",
    ],
  ])('%s fails', async (_name, file, code) => {
    expect(await rules(file, code)).toContain(DYNAMIC);
  });

  it('import() in FightStage.svelte passes', async () => {
    expect(
      await rules(
        'src/lib/components/fight/FightStage.svelte',
        svelte(
          "  void import('@torneo/renderer');\n  void import('@torneo/renderer/placeholders/dummy_a/sheet-0.png?url');",
        ),
      ),
    ).toEqual([]);
  });

  it('a dynamic import of something else passes', async () => {
    expect(
      await rules('src/routes/+page.ts', "export const load = () => import('./other.js');\n"),
    ).toEqual([]);
  });
});

describe('dynamic imports follow every import boundary (TA-11)', () => {
  it.each([
    ['$lib', 'src/routes/+page.ts', "export const load = () => import('$lib/utils');\n"],
    [
      '$env/dynamic/private',
      'src/hooks.server.ts',
      "export const env = () => import('$env/dynamic/private');\n",
    ],
    [
      '$app/environment',
      'src/routes/+layout.ts',
      "export const env = () => import('$app/environment');\n",
    ],
    [
      'feature value in a domain component (#lib)',
      'src/lib/components/fight/Hud.svelte',
      svelte("  void import('#lib/features/system/health.js');"),
    ],
    [
      'feature value in a domain component (relative)',
      'src/lib/components/fight/Hud.svelte',
      svelte("  void import('../../features/x.js');"),
    ],
    [
      'domain component in ui (#lib)',
      'src/lib/components/ui/card/card.svelte',
      svelte("  void import('#lib/components/chat/Chat.svelte');"),
    ],
    [
      'domain component in ui (relative)',
      'src/lib/components/ui/card/card.svelte',
      svelte("  void import('../../chat/Chat.svelte');"),
    ],
    [
      'Paraglide messages in ui (#lib)',
      'src/lib/components/ui/dialog/x.svelte',
      svelte("  void import('#lib/paraglide/messages.js');"),
    ],
    [
      'Paraglide messages in ui (relative)',
      'src/lib/components/ui/dialog/x.svelte',
      svelte("  void import('../../../paraglide/messages.js');"),
    ],
    [
      'the runtime, which exposes setLocale (#lib)',
      'src/routes/+page.ts',
      "export const load = async () => (await import('#lib/paraglide/runtime.js')).setLocale;\n",
    ],
    [
      'the runtime, which exposes setLocale (relative)',
      'src/routes/+page.ts',
      "export const load = () => import('../lib/paraglide/runtime.js');\n",
    ],
    [
      'a template literal without expressions',
      'src/routes/+page.ts',
      'export const load = () => import(`$lib/utils`);\n',
    ],
    [
      'a source that is not a string literal',
      'src/routes/+page.ts',
      'export const load = (name: string) => import(`./x/${name}.js`);\n',
    ],
    [
      'a variable as the source',
      'src/routes/+page.ts',
      "const path = '$lib/utils';\nexport const load = () => import(path);\n",
    ],
  ])('%s fails', async (_name, file, code) => {
    expect(await rules(file, code)).toContain(DYNAMIC);
  });

  it.each([
    [
      'src/lib/components/fight/Hud.svelte',
      svelte("  void import('#lib/features/system/health.js');"),
    ],
    [
      'src/lib/components/ui/card/card.svelte',
      svelte("  void import('#lib/components/chat/Chat.svelte');"),
    ],
    ['src/routes/+page.ts', "export const load = () => import('#lib/paraglide/runtime.js');\n"],
    ['src/routes/+page.ts', "export const load = () => import('@torneo/renderer');\n"],
  ])('each #lib pattern works alone: %s', async (file, code) => {
    expect(await rules(file, code, aliasOnly)).toContain(DYNAMIC);
  });

  it.each([
    [
      'src/routes/+page.ts',
      "export const load = () => import('#lib/features/system/health.js');\n",
    ],
    [
      'src/lib/components/ui/card/card.svelte',
      svelte("  void import('#lib/components/ui/button/index.js');"),
    ],
    ['src/routes/+page.ts', 'export const load = () => import(`./other.js`);\n'],
  ])('allowed elsewhere: %s', async (file, code) => {
    expect(await rules(file, code)).toEqual([]);
  });
});

describe('cases that pass', () => {
  it.each([
    [
      'type import of the renderer in renderer-theme.ts',
      'src/lib/components/fight/renderer-theme.ts',
      "import type { RendererTheme } from '@torneo/renderer';\nexport type T = RendererTheme;\n",
    ],
    [
      'renderer value in FightStage.svelte',
      'src/lib/components/fight/FightStage.svelte',
      svelte(
        "  import { createRenderer } from '@torneo/renderer';\n  const renderer = createRenderer;",
        '<p>{renderer}</p>',
      ),
    ],
    [
      'type import of a feature in a domain component',
      'src/lib/components/fight/Hud.svelte',
      svelte(
        "  import type { Health } from '#lib/features/system/health.js';\n  let { health }: { health: Health } = $props();",
        '<p>{health}</p>',
      ),
    ],
    [
      'Paraglide messages in _a11y-messages.ts',
      'src/lib/components/ui/_a11y-messages.ts',
      "import { m } from '#lib/paraglide/messages.js';\nexport const close = () => m.a11y_close();\n",
    ],
    [
      '$app/env/private in hooks.server.ts',
      'src/hooks.server.ts',
      "import { PUBLIC_ORIGIN } from '$app/env/private';\nexport const x = PUBLIC_ORIGIN;\n",
    ],
    [
      'event.url.pathname in locale-redirect.ts',
      'src/lib/server/hooks/locale-redirect.ts',
      'export const f = (event: { url: URL }) => event.url.pathname + event.url.search;\n',
    ],
    [
      'new URL with one argument',
      'src/lib/env/x.ts',
      'export const f = (value: string) => new URL(value).protocol;\n',
    ],
    [
      'another primitive in ui (#lib and relative)',
      'src/lib/components/ui/dialog/dialog-content.svelte',
      svelte(
        "  import { Button } from '#lib/components/ui/button/index.js';\n  import X from '../button/button.svelte';\n  import Y from './dialog-title.svelte';",
        '<Button /><X /><Y />',
      ),
    ],
    [
      'utils from ui',
      'src/lib/components/ui/button/button.svelte',
      svelte(
        "  import { cn } from '#lib/utils.js';\n  import { cn as c } from '../../../utils.js';",
        "<p class={cn('a', c('b'))}></p>",
      ),
    ],
    [
      'other headers',
      'src/lib/server/hooks/x.ts',
      "export const f = (request: Request) => request.headers.get('origin') ?? request.headers.get('sec-fetch-site');\n",
    ],
    [
      'searchParams.get with a template literal',
      'src/lib/server/hooks/x.ts',
      'export const f = (event: { url: URL }) => event.url.searchParams.get(`theme`);\n',
    ],
    [
      'URL.parse with one value',
      'src/lib/env/x.ts',
      'export const f = (value: string) => URL.parse(value)?.protocol;\n',
    ],
    [
      'other runtime imports',
      'src/routes/+page.ts',
      "import { getLocale } from '#lib/paraglide/runtime.js';\nexport const x = getLocale;\n",
    ],
  ])('%s', async (_name, file, code) => {
    expect(await rules(file, code)).toEqual([]);
  });
});

describe('ui/ climbing to lib/ and back into components/ (TA-10, PR #2 review)', () => {
  it.each([
    [
      'src/lib/components/ui/x.ts',
      "import X from '../../components/fight/X.svelte';\nexport const x = X;\n",
    ],
    [
      'src/lib/components/ui/x.ts',
      "import X from '../../components/chat/X.svelte';\nexport const x = X;\n",
    ],
    [
      'src/lib/components/ui/card/x.ts',
      "import X from '../../../components/chat/X.svelte';\nexport const x = X;\n",
    ],
  ])('%s: %s', async (file, code) => {
    expect(await rules(file, code)).toContain(IMPORTS);
  });

  it('still allows climbing to lib/ and back into components/ui/', async () => {
    expect(
      await rules(
        'src/lib/components/ui/card/x.ts',
        "import { Button } from '../../../components/ui/button/index.js';\nexport const x = Button;\n",
      ),
    ).toEqual([]);
  });
});

describe('ui/ depth (TA-10)', () => {
  it(`no primitive is nested deeper than UI_MAX_DEPTH (${UI_MAX_DEPTH}), where the relative rules stop`, () => {
    const ui = path.join(WEB, 'src/lib/components/ui');
    const deepest = readdirSync(ui, { recursive: true, withFileTypes: true })
      .filter((entry) => entry.isFile())
      .map((entry) => path.relative(ui, entry.parentPath).split(path.sep).filter(Boolean).length)
      .reduce((a, b) => Math.max(a, b), 0);
    expect(deepest).toBeLessThanOrEqual(UI_MAX_DEPTH);
  });

  it.each([
    [
      'src/lib/components/ui/a/b/c/x.ts',
      "import X from '../../../../chat/X.svelte';\nexport const x = X;\n",
    ],
    [
      'src/lib/components/ui/a/x.ts',
      "import X from '../button/../../chat/X.svelte';\nexport const x = X;\n",
    ],
    ['src/lib/components/ui/x.ts', "import X from './../chat/X.svelte';\nexport const x = X;\n"],
    [
      'src/lib/components/ui/x.ts',
      "import X from '#lib/components/ui/../chat/X.svelte';\nexport const x = X;\n",
    ],
  ])('%s: %s', async (file, code) => {
    expect(await rules(file, code)).toContain(IMPORTS);
  });
});
