import type { FightDetail } from '@torneo/protocol';
import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import VECTORS from '../../../server/tests/golden/fight-verify-vectors.json' with { type: 'json' };
import ReplayView from '../../src/lib/components/replay/ReplayView.svelte';
import type { Verdict } from '../../src/lib/features/replay/verify.js';
import { overwriteGetLocale } from '../../src/lib/paraglide/runtime.js';

// 012 RF22 on /fights/{id}: a lost seed has its own notice, and neither the check nor the seed,
// unless the data does not hold.
const DETAIL = VECTORS.find((vector) => vector.name === 'voided_secret_lost')!
  .detail as unknown as FightDetail;

function html(verdict: Verdict | null): string {
  overwriteGetLocale(() => 'en');
  const noop = () => {};
  return render(ReplayView, {
    props: {
      id: DETAIL.fight.id,
      load: 'ready',
      detail: DETAIL,
      verdict,
      action: 'none',
      stageVisible: false,
      decided: false,
      playbackError: false,
      noAnimation: false,
      resultSeen: false,
      result: null,
      roundClock: null,
      names: { a: 'A', b: 'B' },
      source: () => Promise.reject(new Error('no log')),
      onsnapshot: noop,
      onfailure: noop,
      onshowresult: noop,
      onretryverdict: noop,
      onretryload: noop,
      display: (slug: string) => ({ name: slug, epithet: slug }),
      locale: 'en',
    },
  }).body;
}

describe('a fight voided with its seed lost', () => {
  it('says it cannot be verified, without the check or the seed', () => {
    for (const verdict of [null, { status: 'not_applicable' } as const]) {
      const body = html(verdict);
      expect(body).toContain('data-reason="secret_lost"');
      expect(body).toContain('cannot be verified');
      expect(body).not.toContain('data-testid="replay-verdict"');
      expect(body).not.toContain('replay-field-seed');
    }
  });

  it('still shows the check when the data does not hold', () => {
    const body = html({ status: 'mismatch', check: 'commit' });
    expect(body).toContain('data-testid="replay-verdict"');
    expect(body).toContain('data-check="commit"');
  });
});
