import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterAll, describe, expect, it } from 'vitest';
import { GOLDEN_FILE, loadGolden } from '../src/golden.ts';
import { cleanupTmp, tmp } from './support/tmp.ts';

afterAll(cleanupTmp);

const REPO = fileURLToPath(new URL('../../../', import.meta.url));

describe('loadGolden', () => {
  it('reads the real golden', () => {
    const rulesets = loadGolden(REPO);
    expect(rulesets?.find((r) => r.ruleset_version === 1)?.strike_gap_min_ms).toBe(700);
  });

  it.each([
    ['missing', null],
    ['not JSON', '{'],
    ['invalid', '{"versions":[]}'],
  ])('is null when %s', (_name, content) => {
    const root = tmp();
    if (content !== null) {
      const file = path.join(root, ...GOLDEN_FILE.split('/'));
      mkdirSync(path.dirname(file), { recursive: true });
      writeFileSync(file, content);
    }
    expect(loadGolden(root)).toBeNull();
  });
});
