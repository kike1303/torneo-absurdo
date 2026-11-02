import type { FightDetail } from '@torneo/protocol';
import { describe, expect, it, vi } from 'vitest';
import {
  isTrustworthy,
  loadDetail,
  parseFightDetail,
} from '../src/lib/features/replay/load-detail.js';
import {
  CryptoUnavailableError,
  verdictAction,
  verifyFight,
  type Verdict,
  type VerifyDeps,
} from '../src/lib/features/replay/verify.js';
import { readFileSync } from 'node:fs';
import { realDeps, realEngine } from './support/replay-engine.js';

// 011 RF11 to RF13 and RF24: what is not in the shared vectors (replay-vectors.test.ts) because the
// native verifier has nothing to compare it with: the URL, the browser failing and the engine calls.
interface Vector {
  readonly name: string;
  readonly expected_id: number;
  readonly detail: FightDetail;
}
const VECTORS = JSON.parse(
  readFileSync(
    new URL('../../server/tests/golden/fight-verify-vectors.json', import.meta.url),
    'utf8',
  ),
) as readonly Vector[];

function fightOf(name: string): FightDetail {
  const vector = VECTORS.find((row) => row.name === name);
  if (vector === undefined) throw new Error(name);
  return structuredClone(vector.detail);
}

const ID = 12;

describe('verifyFight, beyond the shared vectors', () => {
  it('the id of the response is not the one of the URL: mismatch(data)', async () => {
    expect(await verifyFight(fightOf('ko'), ID + 1, realDeps())).toEqual({
      status: 'mismatch',
      check: 'data',
    });
  });

  it('a voided or cancelled fight never downloads the engine', async () => {
    for (const name of [
      'voided_engine_error',
      'cancelled_missed_window',
      'cancelled_fight_unrunnable',
    ]) {
      const engine = vi.fn(realDeps().engine);
      await verifyFight(fightOf(name), ID, realDeps({ engine }));
      expect(engine, name).not.toHaveBeenCalled();
    }
  });

  it.each(['seed_of_another_fight', 'snapshot_one_byte_off', 'log_null'])(
    'a decided fight that fails before the engine never downloads it: %s',
    async (name) => {
      const engine = vi.fn(realDeps().engine);
      await verifyFight(fightOf(name), ID, realDeps({ engine }));
      expect(engine).not.toHaveBeenCalled();
    },
  );

  it('a rejected engine: engine_unavailable, and it offers a retry', async () => {
    const verdict = await verifyFight(
      fightOf('ko'),
      ID,
      realDeps({ engine: () => Promise.reject(new Error('blocked')) }),
    );
    expect(verdict).toEqual({ status: 'cannot_check', reason: 'engine_unavailable' });
    expect(verdictAction(verdict)).toBe('retry');
  });

  it('a trap of the wasm: engine_failed, and it offers a reload, never a retry', async () => {
    const trap = new WebAssembly.RuntimeError('unreachable');
    const engine = realEngine();
    const verdict = await verifyFight(
      fightOf('ko'),
      ID,
      realDeps({
        engine: async () => ({
          ...engine,
          simulate: () => {
            throw trap;
          },
        }),
      }),
    );
    expect(verdict).toEqual({ status: 'cannot_check', reason: 'engine_failed' });
    expect(verdictAction(verdict)).toBe('reload');
  });

  it('log.encode_failed is the engine failing, not the data', async () => {
    const engine = realEngine();
    const verdict = await verifyFight(
      fightOf('ko'),
      ID,
      realDeps({
        engine: async () => ({
          ...engine,
          simulate: () => {
            throw new Error('log.encode_failed');
          },
        }),
      }),
    );
    expect(verdict).toEqual({ status: 'cannot_check', reason: 'engine_failed' });
  });

  it('an unknown ruleset from the bounds is the same verdict as from simulate', async () => {
    const engine = realEngine();
    const verdict = await verifyFight(
      fightOf('ko'),
      ID,
      realDeps({
        engine: async () => ({
          ...engine,
          rulesetBounds: () => {
            throw new Error('ruleset.unknown');
          },
        }),
      }),
    );
    expect(verdict).toEqual({ status: 'cannot_check', reason: 'ruleset_unknown' });
    expect(verdictAction(verdict)).toBe('reload');
  });

  it('without a secure context: crypto_unavailable, and it offers nothing', async () => {
    const verdict = await verifyFight(
      fightOf('ko'),
      ID,
      realDeps({
        sha256: () => {
          throw new CryptoUnavailableError();
        },
      }),
    );
    expect(verdict).toEqual({ status: 'cannot_check', reason: 'crypto_unavailable' });
    expect(verdictAction(verdict)).toBe('none');
  });

  it('never rejects: any other failure is a cannot_check, not a verdict on the fight', async () => {
    const deps: VerifyDeps = realDeps({
      sha256: () => {
        throw new TypeError('anything');
      },
    });
    await expect(verifyFight(fightOf('ko'), ID, deps)).resolves.toEqual({
      status: 'cannot_check',
      reason: 'engine_failed',
    });
  });

  it('an unknown final offers a reload', () => {
    const verdict: Verdict = { status: 'cannot_check', reason: 'unknown_final' };
    expect(verdictAction(verdict)).toBe('reload');
    expect(verdictAction({ status: 'match', scope: 'fight' })).toBe('none');
    expect(verdictAction({ status: 'mismatch', check: 'log' })).toBe('none');
    expect(verdictAction({ status: 'not_applicable' })).toBe('none');
  });
});

describe('what can be shown of a response (security review of the amendment)', () => {
  it('a real settled fight can', () => {
    expect(isTrustworthy(fightOf('ko'), ID)).toBe(true);
  });

  it('another id, even with a seed and a log, cannot', () => {
    const detail = fightOf('ko');
    expect(detail.seed).not.toBeNull();
    expect(detail.log).not.toBeNull();
    expect(isTrustworthy(detail, ID + 1)).toBe(false);
  });

  it.each(['live', 'betting_closed', 'a_new_state'])(
    'a state %s with a seed and a log cannot',
    (state) => {
      const detail = fightOf('ko') as unknown as { fight: { state: string } };
      detail.fight.state = state;
      expect(isTrustworthy(detail, ID)).toBe(false);
    },
  );

  it('a settled fight without a final cannot', () => {
    const detail = fightOf('ko');
    detail.fight.final = null;
    expect(isTrustworthy(detail, ID)).toBe(false);
  });

  it('what has no shape cannot', () => {
    for (const body of [null, 'x', [], {}, { fight: {} }]) {
      expect(parseFightDetail(body)).toBeNull();
      expect(isTrustworthy(body, ID)).toBe(false);
    }
  });
});

describe('loadDetail', () => {
  const response = (body: BodyInit | null, status = 200) => new Response(body, { status });

  it('reads the fight once, without a cache', async () => {
    const fetcher = vi.fn<typeof fetch>(async () => response('{"a":1}'));
    expect(await loadDetail(12, fetcher)).toEqual({ kind: 'loaded', body: { a: 1 } });
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(fetcher.mock.calls[0]?.[0]).toBe('/api/fights/12');
    expect(fetcher.mock.calls[0]?.[1]?.cache).toBe('no-store');
  });

  it.each([429, 503, 404, 500])('%i is unavailable, never a verdict', async (status) => {
    expect(await loadDetail(12, async () => response('{}', status))).toEqual({
      kind: 'unavailable',
    });
  });

  it('a 200 that is not JSON is unavailable', async () => {
    expect(await loadDetail(12, async () => response('<html>'))).toEqual({ kind: 'unavailable' });
  });

  it('the network failing is unavailable', async () => {
    expect(
      await loadDetail(12, async () => {
        throw new TypeError('network');
      }),
    ).toEqual({ kind: 'unavailable' });
  });
});
