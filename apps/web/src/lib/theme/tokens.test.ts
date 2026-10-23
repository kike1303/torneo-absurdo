import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { clampRgb, converter, parse, wcagContrast } from 'culori';
import { describe, expect, it } from 'vitest';
import { THEME_COLOR } from './theme-color.js';

// ADR-0009 test 3: contrast of the semantic tokens in both themes, and theme-color.ts.
const css = readFileSync(fileURLToPath(new URL('./tokens.css', import.meta.url)), 'utf8');

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

function color(theme: Record<string, string>, name: string): string {
  let value = theme[`--${name}`];
  for (let i = 0; value?.startsWith('var(') && i < 5; i++) {
    const ref = /^var\((--[\w-]+)\)$/.exec(value)?.[1];
    value = ref ? (theme[ref] ?? primitives[ref]) : undefined;
  }
  if (!value || !parse(value)) throw new Error(`cannot resolve --${name}: ${value}`);
  return value;
}

const TEXT_PAIRS: [string, string][] = [
  ['foreground', 'background'],
  ['card-foreground', 'card'],
  ['popover-foreground', 'popover'],
  ['primary-foreground', 'primary'],
  ['secondary-foreground', 'secondary'],
  ['muted-foreground', 'background'],
  ['muted-foreground', 'muted'],
  ['muted-foreground', 'card'],
  ['accent-foreground', 'accent'],
  ['destructive-foreground', 'destructive'],
  ['arena-text', 'arena-bg'],
];
const UI_PAIRS: [string, string][] = [
  ['ring', 'background'],
  ['input', 'background'],
  ['arena-hp', 'arena-bg'],
  ['arena-hp', 'arena-hp-lost'],
];

const toRgb = converter('rgb');

describe.each(Object.entries(themes))('tokens (%s)', (name, theme) => {
  it.each(TEXT_PAIRS)('%s on %s ≥ 4.5:1', (fg, bg) => {
    expect(wcagContrast(color(theme, fg), color(theme, bg))).toBeGreaterThanOrEqual(4.5);
  });

  it.each(UI_PAIRS)('%s on %s ≥ 3:1', (fg, bg) => {
    expect(wcagContrast(color(theme, fg), color(theme, bg))).toBeGreaterThanOrEqual(3);
  });

  it('theme-color.ts matches --background within ±1 per channel', () => {
    const expected = clampRgb(toRgb(color(theme, 'background'))!);
    const actual = toRgb(THEME_COLOR[name as 'light' | 'dark'])!;
    for (const channel of ['r', 'g', 'b'] as const) {
      expect(
        Math.abs(Math.round(expected[channel] * 255) - Math.round(actual[channel] * 255)),
      ).toBeLessThanOrEqual(1);
    }
  });
});
