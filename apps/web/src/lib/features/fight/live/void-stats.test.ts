import { describe, expect, it, vi } from 'vitest';
import { MAX_READ_DELAY_MS, createVoidStatsReader, parseVoidStats } from './void-stats.js';

const answer = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

function deps(fetchFn: typeof fetch, random = 0.5) {
  const wait = vi.fn(() => Promise.resolve());
  return { fetchFn, random: () => random, wait };
}

describe('parseVoidStats', () => {
  it('takes only the shape of the protocol', () => {
    expect(parseVoidStats({ voided: 4, secret_lost: 1 })).toEqual({ voided: 4, secret_lost: 1 });
    for (const bad of [null, 'x', {}, { voided: 1 }, { voided: '1', secret_lost: 1 }]) {
      expect(parseVoidStats(bad)).toBeNull();
    }
    expect(parseVoidStats({ voided: 1.5, secret_lost: 1 })).toBeNull();
  });
});

describe('createVoidStatsReader', () => {
  it('reads once per fight, after its wait', async () => {
    const fetchFn = vi.fn<typeof fetch>(() =>
      Promise.resolve(answer({ voided: 4, secret_lost: 1 })),
    );
    const d = deps(fetchFn, 0.5);
    const reader = createVoidStatsReader(null, d);
    await expect(reader.read(7)).resolves.toEqual({ voided: 4, secret_lost: 1 });
    await expect(reader.read(7)).resolves.toBeNull();
    expect(fetchFn).toHaveBeenCalledTimes(1);
    expect(fetchFn).toHaveBeenCalledWith('/api/fights/void-stats', expect.anything());
    expect(d.wait).toHaveBeenCalledWith(MAX_READ_DELAY_MS / 2);
    await expect(reader.read(8)).resolves.toEqual({ voided: 4, secret_lost: 1 });
    expect(fetchFn).toHaveBeenCalledTimes(2);
  });

  it('does not read what the SSR already read', async () => {
    const fetchFn = vi.fn<typeof fetch>();
    const reader = createVoidStatsReader(7, deps(fetchFn));
    await expect(reader.read(7)).resolves.toBeNull();
    expect(fetchFn).not.toHaveBeenCalled();
  });

  it('a failure leaves out the counter, and it does not try again', async () => {
    for (const fetchFn of [
      vi.fn<typeof fetch>(() => Promise.reject(new Error('down'))),
      vi.fn<typeof fetch>(() => Promise.resolve(answer({ error: 'fight.unavailable' }, 503))),
      vi.fn<typeof fetch>(() => Promise.resolve(answer({ other: 1 }))),
    ]) {
      const reader = createVoidStatsReader(null, deps(fetchFn));
      await expect(reader.read(7)).resolves.toBeNull();
      await expect(reader.read(7)).resolves.toBeNull();
      expect(fetchFn).toHaveBeenCalledTimes(1);
    }
  });
});
