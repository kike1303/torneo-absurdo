import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

// ADR-0012 B4.6 (2026-10-30 amendment, recommendations 1 and 2 of security-reviewer): with
// form-action widened to the providers' authorize endpoints, a form of ours must carry nothing
// those endpoints use, and no route of web may redirect to those origins.
const SRC = fileURLToPath(new URL('../src', import.meta.url));
const FIELDS = /\bname\s*=\s*\{?\s*["'`]?(state|client_id|redirect_uri)["'`]?/;
const ORIGINS = /accounts\.google\.com|discord\.com/;

function walk(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name !== 'paraglide') walk(full, out);
    } else if (/\.(svelte|ts|js)$/.test(entry.name) && !entry.name.endsWith('.test.ts')) {
      out.push(full);
    }
  }
  return out;
}

const files = walk(SRC).map((full) => ({
  file: path.relative(SRC, full),
  text: readFileSync(full, 'utf8'),
}));

describe('B4.6: forms and redirects of web', () => {
  it('scans the source tree (positive control)', () => {
    expect(files.some((f) => f.file.endsWith('login/+page.svelte'))).toBe(true);
  });

  it('no form field is named state, client_id or redirect_uri', () => {
    expect(files.filter((f) => FIELDS.test(f.text)).map((f) => f.file)).toEqual([]);
  });

  it('no file of src/ names the providers origins', () => {
    expect(files.filter((f) => ORIGINS.test(f.text)).map((f) => f.file)).toEqual([]);
  });

  it.each([
    ['<input type="hidden" name="state" value="x" />', true],
    ["<input name={'client_id'} />", true],
    ['<input name=redirect_uri>', true],
    ['<input name="next" />', false],
  ])('the field pattern on %s → %s', (text, expected) => {
    expect(FIELDS.test(text)).toBe(expected);
  });
});
