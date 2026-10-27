// Applies the JSON Pointer operations of a rule fixture to a copy of a valid document.
import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';

export interface PatchOp {
  readonly op: 'set' | 'delete' | 'insert' | 'push';
  readonly path: string;
  readonly value?: unknown;
}

export interface RuleFixture {
  readonly name: string;
  readonly ops: readonly PatchOp[];
  readonly expect: { readonly code: string; readonly path: string };
  readonly options?: { readonly expectedId?: string };
  readonly manifests?: boolean;
}

function segments(pointer: string): string[] {
  return pointer
    .split('/')
    .slice(1)
    .map((s) => s.replaceAll('~1', '/').replaceAll('~0', '~'));
}

type Container = Record<string, unknown> | unknown[];

export function applyOps(doc: unknown, ops: readonly PatchOp[]): unknown {
  const out: unknown = structuredClone(doc);
  for (const op of ops) {
    const keys = segments(op.path);
    const last = keys.pop() ?? '';
    let parent = out as Container;
    for (const key of keys) parent = (parent as Record<string, unknown>)[key] as Container;
    if (op.op === 'push') {
      ((parent as Record<string, unknown>)[last] as unknown[]).push(op.value);
    } else if (Array.isArray(parent)) {
      const i = Number(last);
      if (op.op === 'delete') parent.splice(i, 1);
      else if (op.op === 'insert') parent.splice(i, 0, op.value);
      else parent[i] = op.value;
    } else if (op.op === 'delete') {
      delete parent[last];
    } else {
      parent[last] = op.value;
    }
  }
  return out;
}

/** A valid variant of `_valid.json` (`_valid.<name>.json`), as operations on it. */
export interface ValidVariant {
  readonly name: string;
  readonly ops: readonly PatchOp[];
}

export function loadFixtures(dir: string): {
  valid: unknown;
  variants: ValidVariant[];
  rules: RuleFixture[];
} {
  const files = readdirSync(dir).filter((f) => f.endsWith('.json'));
  const read = (f: string): unknown => JSON.parse(readFileSync(path.join(dir, f), 'utf8'));
  const name = (f: string) => f.replace(/\.json$/, '');
  return {
    valid: read('_valid.json'),
    variants: files
      .filter((f) => f.startsWith('_valid.') && f !== '_valid.json')
      .map((f) => ({ name: name(f).slice('_valid.'.length), ...(read(f) as { ops: PatchOp[] }) })),
    rules: files
      .filter((f) => !f.startsWith('_valid.'))
      .map((f) => ({ name: name(f), ...(read(f) as Omit<RuleFixture, 'name'>) })),
  };
}
