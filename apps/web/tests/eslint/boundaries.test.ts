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
  ])('absolute URL: %s', async (_name, code) => {
    expect(await rules('src/lib/server/hooks/x.ts', code)).toContain(SYNTAX);
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
      'other runtime imports',
      'src/routes/+page.ts',
      "import { getLocale } from '#lib/paraglide/runtime.js';\nexport const x = getLocale;\n",
    ],
  ])('%s', async (_name, file, code) => {
    expect(await rules(file, code)).toEqual([]);
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
