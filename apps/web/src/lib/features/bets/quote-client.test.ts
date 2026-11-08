import { describe, expect, it } from 'vitest';
import type { BetQuote } from '@torneo/protocol';
import { QUOTE_DEBOUNCE_MS, createQuoteClient, type QuoteState } from './quote-client.js';

// 012 RF47, plan-c C4.
function quote(amount: number, side: 'a' | 'b' = 'a'): BetQuote {
  return {
    fight_id: 2,
    side,
    amount,
    payout: amount * 2,
    net: amount,
    one_sided: false,
    rake_bp: 0,
  };
}

function harness(answer: (url: string) => Promise<Response>) {
  const states: QuoteState[] = [];
  const timers: { run: () => void; ms: number; cleared: boolean }[] = [];
  const urls: string[] = [];
  const client = createQuoteClient({
    fetchFn: ((url: string) => {
      urls.push(url);
      return answer(url);
    }) as unknown as typeof fetch,
    setTimer: (run, ms) => {
      const timer = { run, ms, cleared: false };
      timers.push(timer);
      return timer;
    },
    clearTimer: (timer) => {
      (timer as { cleared: boolean }).cleared = true;
    },
    onChange: (state) => states.push(state),
  });
  const fire = () => {
    for (const timer of timers) if (!timer.cleared) timer.run();
    timers.length = 0;
  };
  const last = () => states.at(-1);
  return { client, fire, urls, last, states };
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
const ok = (body: unknown) => Promise.resolve(Response.json(body));

describe('the quote', () => {
  it('asks once, after the wait, for the last amount typed', async () => {
    const h = harness((url) =>
      ok(quote(Number(new URL(url, 'http://x').searchParams.get('amount')))),
    );
    h.client.request({ fightId: 2, side: 'a', amount: 2 });
    h.client.request({ fightId: 2, side: 'a', amount: 25 });
    h.fire();
    await flush();
    expect(h.urls).toEqual(['/api/fights/2/quote?side=a&amount=25']);
    expect(h.last()).toEqual({
      status: 'ready',
      input: { fightId: 2, side: 'a', amount: 25 },
      quote: quote(25),
    });
    expect(QUOTE_DEBOUNCE_MS).toBe(300);
  });

  it('never shows an answer for another amount', async () => {
    let release: (r: Response) => void = () => undefined;
    const h = harness(() => new Promise((resolve) => (release = resolve)));
    h.client.request({ fightId: 2, side: 'a', amount: 25 });
    h.fire();
    h.client.request({ fightId: 2, side: 'a', amount: 30 });
    release(Response.json(quote(25)));
    await flush();
    expect(h.states.some((s) => s.status === 'ready')).toBe(false);
  });

  it('a pool_update marks it stale and asks nothing; refresh asks again', async () => {
    const h = harness(() => ok(quote(25)));
    h.client.request({ fightId: 2, side: 'a', amount: 25 });
    h.fire();
    await flush();
    h.client.poolChanged(3);
    expect(h.last()?.status).toBe('ready');
    h.client.poolChanged(2);
    expect(h.last()?.status).toBe('stale');
    expect(h.urls).toHaveLength(1);
    h.client.refresh();
    await flush();
    expect(h.urls).toHaveLength(2);
    expect(h.last()?.status).toBe('ready');
  });

  it('a pool_update while asking makes the answer stale when it arrives', async () => {
    let release: (r: Response) => void = () => undefined;
    const h = harness(() => new Promise((resolve) => (release = resolve)));
    h.client.request({ fightId: 2, side: 'a', amount: 25 });
    h.fire();
    h.client.poolChanged(2);
    release(Response.json(quote(25)));
    await flush();
    expect(h.last()?.status).toBe('stale');
  });

  it('a 422 below the minimum is a hint, not a refusal', async () => {
    const h = harness(() =>
      Promise.resolve(Response.json({ error: 'bet.below_minimum' }, { status: 422 })),
    );
    h.client.request({ fightId: 2, side: 'a', amount: 3 });
    h.fire();
    await flush();
    expect(h.last()?.status).toBe('below_minimum');
  });

  it.each([
    [
      'an error',
      () => Promise.resolve(Response.json({ error: 'bet.window_closed' }, { status: 409 })),
    ],
    ['a 429', () => Promise.resolve(new Response('', { status: 429 }))],
    ['the network', () => Promise.reject(new Error('down'))],
    ['a body of another shape', () => ok({ payout: 'x' })],
  ])('%s: no estimate', async (_, answer) => {
    const h = harness(answer);
    h.client.request({ fightId: 2, side: 'b', amount: 25 });
    h.fire();
    await flush();
    expect(h.last()?.status).toBe('unavailable');
  });

  it('an empty amount clears it without asking', () => {
    const h = harness(() => ok(quote(25)));
    h.client.request({ fightId: 2, side: 'a', amount: 0 });
    h.client.request(null);
    h.fire();
    expect(h.urls).toEqual([]);
    expect(h.last()).toEqual({ status: 'idle' });
  });
});
