import { mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import lore from 'virtual:fighter-lore';
import { DEFAULT_CHARACTERS_DIR, readFighterLore } from '../vite/fighter-lore.js';
import { fighterDisplay } from '../src/lib/features/fight/live/fighter-names.js';

// 010 RF56: the two fields, for every character and locale, without copying them to the messages.
const dirs: string[] = [];
afterEach(() => {
  for (const dir of dirs.splice(0)) rmSync(dir, { recursive: true });
});

function characters(files: Record<string, string>): string {
  const dir = mkdtempSync(path.join(tmpdir(), 'lore-'));
  dirs.push(dir);
  for (const [name, text] of Object.entries(files)) {
    const file = path.join(dir, name);
    mkdirSync(path.dirname(file), { recursive: true });
    writeFileSync(file, text);
  }
  return dir;
}

const good = (name: string) => `---\nname: ${name}\nepithet: The ${name}\nsecret: no\n---\nBody`;

describe('the lore of the fighters', () => {
  it('every character of content/ has a name and an epithet in both locales', () => {
    const slugs = readdirSync(DEFAULT_CHARACTERS_DIR, { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .map((entry) => entry.name)
      .sort();
    expect(slugs.length).toBeGreaterThan(0);
    expect(Object.keys(lore).sort()).toEqual(slugs);
    for (const slug of slugs) {
      for (const locale of ['en', 'es'] as const) {
        expect(lore[slug]?.[locale].name, `${slug} ${locale}`).not.toBe('');
        expect(lore[slug]?.[locale].epithet, `${slug} ${locale}`).not.toBe('');
      }
    }
  });

  it('only those two fields reach the module', () => {
    const dir = characters({ 'x/lore.en.md': good('A'), 'x/lore.es.md': good('B') });
    expect(readFighterLore(dir)).toEqual({
      x: { en: { name: 'A', epithet: 'The A' }, es: { name: 'B', epithet: 'The B' } },
    });
  });

  it.each([
    ['no epithet', '---\nname: A\n---\n'],
    ['no name', '---\nepithet: A\n---\n'],
    ['no frontmatter', 'Body'],
    ['an empty name', '---\nname: ""\nepithet: A\n---\n'],
  ])('fails with %s', (_name, text) => {
    const dir = characters({ 'x/lore.en.md': text, 'x/lore.es.md': good('B') });
    expect(() => readFighterLore(dir)).toThrow(/name and an epithet/);
  });

  it('fails without the file of a locale', () => {
    const dir = characters({ 'x/lore.en.md': good('A') });
    expect(() => readFighterLore(dir)).toThrow();
  });
});

describe('fighterDisplay', () => {
  it('gives the lore of a known slug in the locale', () => {
    const slug = Object.keys(lore)[0] as string;
    expect(fighterDisplay(slug, 'es')).toEqual(lore[slug]?.es);
    expect(fighterDisplay(slug, 'en')).toEqual(lore[slug]?.en);
  });

  it('an unknown slug has the fallback with the slug and no epithet', () => {
    expect(fighterDisplay('striker', 'es')).toEqual({ name: 'Luchador striker', epithet: null });
    expect(fighterDisplay('striker', 'en')).toEqual({ name: 'Fighter striker', epithet: null });
  });

  it('a name of the prototype is not a fighter', () => {
    expect(fighterDisplay('constructor', 'en').epithet).toBeNull();
  });
});
