// The name and the epithet of each fighter (010 RF56), read at build time from the frontmatter of
// content/characters/<slug>/lore.<locale>.md. They are never copied to apps/web/messages/: those
// files are under AGPL, and LICENSE-CONTENT.md reserves all of content/, names included. Only these
// two fields reach the bundle.
import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Plugin } from 'vite';
import { parse } from 'yaml';

export const FIGHTER_LORE_ID = 'virtual:fighter-lore';
const RESOLVED_ID = `\0${FIGHTER_LORE_ID}`;
/** `<repo>/content/characters`, from this file: never from the root Vite was started with. */
export const DEFAULT_CHARACTERS_DIR = path.resolve(
  fileURLToPath(new URL('../../../content/characters/', import.meta.url)),
);
const FRONTMATTER = /^---\r?\n([\s\S]*?)\r?\n---/;

export interface FighterText {
  readonly name: string;
  readonly epithet: string;
}

type LoreLocale = 'en' | 'es';

export type FighterLore = Record<string, Record<LoreLocale, FighterText>>;

function fieldsOf(text: string, file: string): FighterText {
  const block = FRONTMATTER.exec(text)?.[1];
  const data: unknown = block === undefined ? undefined : parse(block);
  const { name, epithet } = (data ?? {}) as { name?: unknown; epithet?: unknown };
  if (typeof name !== 'string' || name === '' || typeof epithet !== 'string' || epithet === '') {
    throw new Error(`${file}: the frontmatter must have a name and an epithet`);
  }
  return { name, epithet };
}

/** Throws if a character lacks a field in a locale: the build fails instead of showing a gap. */
export function readFighterLore(dir = DEFAULT_CHARACTERS_DIR): FighterLore {
  const lore: FighterLore = {};
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const text = (locale: LoreLocale): FighterText => {
      const file = path.join(dir, entry.name, `lore.${locale}.md`);
      return fieldsOf(readFileSync(file, 'utf8'), file);
    };
    lore[entry.name] = { en: text('en'), es: text('es') };
  }
  return lore;
}

export function fighterLore(dir = DEFAULT_CHARACTERS_DIR): Plugin {
  return {
    name: 'torneo-fighter-lore',
    resolveId(id) {
      return id === FIGHTER_LORE_ID ? RESOLVED_ID : null;
    },
    load(id) {
      return id === RESOLVED_ID ? `export default ${JSON.stringify(readFighterLore(dir))};` : null;
    },
  };
}
