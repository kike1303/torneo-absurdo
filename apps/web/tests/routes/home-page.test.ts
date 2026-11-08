import type { FightSummary, FightsCurrent } from '@torneo/protocol';
import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import { overwriteGetLocale } from '../../src/lib/paraglide/runtime.js';
import Page from '../../src/routes/+page.svelte';

// 012 RF22, the path of the SSR: the e2e cannot reach it, because its fake server only answers to
// the browser and a real `secret_lost` needs the CLI.
const LAST = {
  id: 7,
  state: 'settled',
  a: { slug: 'fontanero-saltarin', max_hp: 100 },
  b: { slug: 'guerrero-erizado', max_hp: 100 },
  final: { kind: 'voided', reason: 'secret_lost' },
} as FightSummary;
const BOARD: FightsCurrent = {
  server_time: '0',
  halted: false,
  current: null,
  next: null,
  last: LAST,
};

function html(voidStats: { voided: number; secret_lost: number } | null): string {
  overwriteGetLocale(() => 'es');
  return render(Page, {
    props: {
      data: {
        board: BOARD,
        voidStats,
        debug: false,
        betting: { signedIn: false, wallet: null, mine: {}, pool: null, clientKey: 'k' },
      },
      form: null,
    },
  } as never).body;
}

describe('the home page of a last fight voided with its secret lost', () => {
  it('comes with the counter that the SSR read', () => {
    const body = html({ voided: 4, secret_lost: 2 });
    expect(body).toContain('data-result="voided_unverifiable"');
    expect(body).toContain('Peleas anuladas: 4. Sin poder verificar: 2');
  });

  it('if the SSR could not read it, comes with the text and without the line', () => {
    const body = html(null);
    expect(body).toContain('data-result="voided_unverifiable"');
    expect(body).not.toContain('fight-result-void-count');
  });
});
