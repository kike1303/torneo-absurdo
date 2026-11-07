import type { FightSummary } from '@torneo/protocol';
import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import FightResultCard from '../../src/lib/components/fight/FightResultCard.svelte';
import type { Locale } from '../../src/lib/i18n/locale.js';
import { overwriteGetLocale } from '../../src/lib/paraglide/runtime.js';

// 012 RF22 and ADR-0007: the counter of a fight that cannot be verified, as the server sends it.
const FIGHT = { id: 7, a: { slug: 'a', max_hp: 100 }, b: { slug: 'b', max_hp: 100 } };

function html(locale: Locale, voidStats: { voided: number; secret_lost: number } | null): string {
  overwriteGetLocale(() => locale);
  return render(FightResultCard, {
    props: {
      result: {
        fight: FIGHT as FightSummary,
        kind: 'voided_unverifiable',
        winner: null,
      },
      previous: true,
      display: (slug: string) => ({ name: slug, epithet: slug }),
      locale,
      voidStats,
    },
  }).body;
}

describe('the counter of FightResultCard', () => {
  it('formats its numbers with the grouping of the locale', () => {
    expect(html('en', { voided: 1234, secret_lost: 1000 })).toContain(
      'Voided fights: 1,234. Unverifiable: 1,000',
    );
    expect(html('es', { voided: 12345, secret_lost: 2 })).toContain(
      'Peleas anuladas: 12.345. Sin poder verificar: 2',
    );
  });

  it('without the counter, there is no line and the text stays', () => {
    const body = html('es', null);
    expect(body).not.toContain('fight-result-void-count');
    expect(body).toContain('no se puede verificar');
  });
});
