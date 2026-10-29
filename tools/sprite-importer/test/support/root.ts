// A minimal repository root in a temporary directory: the real ruleset golden and nothing else.
import { copyFileSync, cpSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { GOLDEN_FILE } from '../../src/golden.ts';
import { tmp } from './tmp.ts';

export const REPO = fileURLToPath(new URL('../../../../', import.meta.url));
export const EXPORT_REF = fileURLToPath(new URL('../../fixtures/export-ref/', import.meta.url));

export function goldenRoot(): string {
  const root = tmp('sprite-root-');
  const to = path.join(root, ...GOLDEN_FILE.split('/'));
  mkdirSync(path.dirname(to), { recursive: true });
  copyFileSync(path.join(REPO, ...GOLDEN_FILE.split('/')), to);
  return root;
}

/** A writable copy of the reference export. */
export function copyExportRef(): string {
  const dir = tmp('sprite-export-');
  cpSync(EXPORT_REF, dir, { recursive: true });
  return dir;
}

export function readExportJson(dir: string): Record<string, unknown> {
  return JSON.parse(readFileSync(path.join(dir, 'export.json'), 'utf8')) as Record<string, unknown>;
}

export function writeExportJson(dir: string, json: unknown): void {
  writeFileSync(path.join(dir, 'export.json'), JSON.stringify(json, null, 2) + '\n');
}
