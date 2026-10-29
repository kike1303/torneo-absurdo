// Fixture roots for `check` (007 plan-b decision 31), built in a temporary directory without
// committed PNG: the real golden, content/sprites/ref_a imported from fixtures/export-ref with a
// valid provenance.json, content/sprites/README.md and one character that links ref_a. Every
// case copies the template and changes one thing.
import { cpSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { runImport } from '../../src/import.ts';
import { EXPORT_REF, goldenRoot } from './root.ts';
import { tmp } from './tmp.ts';

export const SET = ['content', 'sprites', 'ref_a'] as const;
export const CHARACTER = ['content', 'characters', 'hero-one'] as const;

export const PROVENANCE = Object.freeze({
  format: 'torneo.provenance',
  format_version: 1,
  set: 'ref_a',
  author: 'owner',
  origin: 'generated',
  date: '2026-10-28',
  ai_assisted: false,
  license: 'content',
});

export const json = (value: unknown) => JSON.stringify(value, null, 2) + '\n';

export function writeCharacter(root: string, slug: string, body: Record<string, unknown>): void {
  const dir = path.join(root, 'content', 'characters', slug);
  mkdirSync(dir, { recursive: true });
  writeFileSync(path.join(dir, 'character.json'), json({ slug, ...body }));
}

let template: string | undefined;

function buildTemplate(): string {
  const root = goldenRoot();
  const result = runImport({ root, inDir: EXPORT_REF, outDir: path.join(root, ...SET) });
  if (result.status !== 0) throw new Error(JSON.stringify(result.issues));
  writeFileSync(path.join(root, ...SET, 'provenance.json'), json(PROVENANCE));
  writeFileSync(path.join(root, 'content', 'sprites', 'README.md'), '# Sprites\n');
  writeCharacter(root, 'hero-one', { sprites: 'ref_a' });
  return root;
}

/** A fresh copy of the valid root (the template is imported once per test file). */
export function validRoot(): string {
  template ??= buildTemplate();
  const root = tmp('sprite-check-');
  cpSync(template, root, { recursive: true, verbatimSymlinks: true });
  return root;
}
