import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { converter, parse } from 'culori';
import { describe, expect, it } from 'vitest';
import { buildRendererTheme, tokenToHex } from './renderer-theme.js';

// ADR-0009 D7: the renderer theme comes from tokens.css, converted with culori/fn.
const css = readFileSync(fileURLToPath(new URL('../../theme/tokens.css', import.meta.url)), 'utf8');

function block(selector: string): Record<string, string> {
  const start = css.indexOf(`${selector} {`);
  if (start === -1) throw new Error(`missing block ${selector}`);
  const body = css.slice(css.indexOf('{', start) + 1, css.indexOf('}', start));
  const vars: Record<string, string> = {};
  for (const match of body.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g))
    vars[match[1]!] = match[2]!.trim();
  return vars;
}

const primitives = block(':root');
const themes = {
  light: block(":root,\n[data-theme='light']"),
  dark: block("[data-theme='dark']"),
};

/** What getComputedStyle would give: the token with its var() references resolved. */
function reader(theme: Record<string, string>) {
  return (name: string): string => {
    let value = theme[name] ?? primitives[name];
    for (let i = 0; value?.startsWith('var(') && i < 5; i++) {
      const ref = /^var\((--[\w-]+)\)$/.exec(value)?.[1];
      value = ref ? (theme[ref] ?? primitives[ref]) : undefined;
    }
    return value ?? '';
  };
}

const toRgb = converter('rgb');
const channels = (hex: number) => [(hex >> 16) & 255, (hex >> 8) & 255, hex & 255];

describe.each(Object.entries(themes))('buildRendererTheme (%s)', (_name, theme) => {
  const read = reader(theme);
  const built = buildRendererTheme(read);

  it.each([
    ['bg', '--arena-bg'],
    ['hp', '--arena-hp'],
    ['hpLost', '--arena-hp-lost'],
    ['crit', '--arena-crit'],
    ['text', '--arena-text'],
  ] as const)('%s matches %s in sRGB within ±1 per channel', (role, token) => {
    const expected = toRgb(parse(read(token))!)!;
    const actual = channels(built.colors[role]);
    (['r', 'g', 'b'] as const).forEach((c, i) => {
      const want = Math.round(Math.min(1, Math.max(0, expected[c])) * 255);
      expect(Math.abs((actual[i] ?? -1) - want)).toBeLessThanOrEqual(1);
    });
  });

  it('every color is a 24-bit integer', () => {
    for (const value of Object.values(built.colors)) {
      expect(Number.isInteger(value)).toBe(true);
      expect(value).toBeGreaterThanOrEqual(0);
      expect(value).toBeLessThanOrEqual(0xffffff);
    }
  });
});

describe('fonts and errors', () => {
  it('falls back to sans-serif when a font token is empty', () => {
    const read = reader(themes.dark);
    const theme = buildRendererTheme((name) => (name.startsWith('--font') ? '' : read(name)));
    expect(theme.fonts).toEqual({ sans: 'sans-serif', display: 'sans-serif' });
  });

  it('rejects a value that is not a color', () => {
    expect(() => tokenToHex('not a color')).toThrow('renderer_theme.invalid_color');
  });
});
