import { describe, expect, it } from 'vitest';
import { createSharedCache } from '../src/stage/shared-cache.ts';

// QA F1/F2 of A2: Pixi's Assets cache is global, so overlapping renderers share each URL.
function fake() {
  const log: string[] = [];
  let fail = false;
  const cache = createSharedCache<string>({
    load: async (url) => {
      log.push(`load ${url}`);
      await Promise.resolve();
      if (fail) throw new Error('boom');
      return `tex:${url}`;
    },
    unload: async (url) => {
      log.push(`unload ${url}`);
      await Promise.resolve();
    },
  });
  return { cache, log, failNext: (v: boolean) => (fail = v) };
}

describe('shared cache of loaded URLs', () => {
  it('loads once and unloads after the last release', async () => {
    const { cache, log } = fake();
    expect(await cache.acquire('a')).toBe('tex:a');
    expect(await cache.acquire('a')).toBe('tex:a');
    await cache.release('a');
    expect(log).toEqual(['load a']);
    await cache.release('a');
    expect(log).toEqual(['load a', 'unload a']);
  });

  it('a release of an old owner never unloads what a new owner uses', async () => {
    const { cache, log } = fake();
    await cache.acquire('a');
    const next = cache.acquire('a');
    await cache.release('a');
    expect(await next).toBe('tex:a');
    expect(log).toEqual(['load a']);
  });

  it('an acquire during an unload waits for it and loads again', async () => {
    const { cache, log } = fake();
    await cache.acquire('a');
    const unloading = cache.release('a');
    const again = cache.acquire('a');
    await unloading;
    expect(await again).toBe('tex:a');
    expect(log).toEqual(['load a', 'unload a', 'load a']);
  });

  it('an acquire during an unload does not load until the unload ends', async () => {
    const log: string[] = [];
    let finishUnload = () => {};
    const cache = createSharedCache<string>({
      load: async (url) => {
        log.push(`load ${url}`);
        return `tex:${url}`;
      },
      unload: (url) => {
        log.push(`unload ${url}`);
        return new Promise((resolve) => {
          finishUnload = resolve;
        });
      },
    });
    // A macrotask runs only after every pending microtask, so nothing here depends on timing.
    const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

    await cache.acquire('a');
    const unloading = cache.release('a');
    const again = cache.acquire('a');
    await settle();
    expect(log).toEqual(['load a', 'unload a']);

    finishUnload();
    await unloading;
    expect(await again).toBe('tex:a');
    expect(log).toEqual(['load a', 'unload a', 'load a']);
  });

  it('a failed load is forgotten without unloading (F2)', async () => {
    const { cache, log, failNext } = fake();
    failNext(true);
    await expect(cache.acquire('a')).rejects.toThrow('boom');
    await cache.release('a');
    failNext(false);
    expect(await cache.acquire('a')).toBe('tex:a');
    expect(log).toEqual(['load a', 'load a']);
  });

  it('releasing an unknown URL does nothing', async () => {
    const { cache, log } = fake();
    await cache.release('nope');
    expect(log).toEqual([]);
  });
});
