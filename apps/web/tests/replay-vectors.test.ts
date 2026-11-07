import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { verifyFight, type Verdict } from '../src/lib/features/replay/verify.js';
import { realDeps } from './support/replay-engine.js';

// 011 RF14: the same table as the native verifier (apps/server/tests/fights_public_verify.rs, which
// writes it), with the real `.wasm`. `Ok` is a `match` or `not_applicable`, and `Err` a `mismatch`,
// but for the two exceptions marked in the file: a build that does not know something is no evidence.
interface Vector {
  readonly name: string;
  readonly expected_id: number;
  readonly detail: unknown;
  readonly native: 'ok' | 'err' | 'undeserializable';
  readonly browser: string;
  readonly exception: boolean;
}

const VECTORS = JSON.parse(
  readFileSync(
    new URL('../../server/tests/golden/fight-verify-vectors.json', import.meta.url),
    'utf8',
  ),
) as readonly Vector[];

function label(verdict: Verdict): string {
  switch (verdict.status) {
    case 'match':
      return `match:${verdict.scope}`;
    case 'mismatch':
      return `mismatch:${verdict.check}`;
    case 'cannot_check':
      return `cannot_check:${verdict.reason}`;
    case 'not_applicable':
      return 'not_applicable';
  }
}

describe('the verification vectors', () => {
  it('are all there, so this never passes empty', () => {
    expect(VECTORS).toHaveLength(71);
    const names = VECTORS.map((vector) => vector.name);
    expect(new Set(names).size).toBe(names.length);
    for (const name of [
      'ko',
      'decision',
      'draw',
      'voided_engine_error',
      'cancelled_missed_window',
      'cancelled_fight_unrunnable',
      'log_one_byte_off',
      'seed_of_another_fight',
      'seed_commit_one_byte_off',
      'final_null',
      'state_live',
      'unknown_ruleset_with_its_hash',
      'unknown_final_kind',
      'unknown_voided_reason_without_a_seed',
    ]) {
      expect(names).toContain(name);
    }
  });

  it.each(VECTORS.map((vector) => [vector.name, vector] as const))('%s', async (_name, vector) => {
    const verdict = await verifyFight(vector.detail, vector.expected_id, realDeps());
    expect(label(verdict)).toBe(vector.browser);
  });

  it('agrees with the native result, but for the marked exceptions', () => {
    for (const vector of VECTORS) {
      if (vector.exception) {
        expect(vector.browser, vector.name).toMatch(/^cannot_check:/);
        expect(vector.native, vector.name).not.toBe('ok');
      } else if (vector.native === 'ok') {
        expect(vector.browser, vector.name).toMatch(/^(?:match:|not_applicable$)/);
      } else {
        expect(vector.browser, vector.name).toMatch(/^mismatch:/);
      }
    }
  });
});
