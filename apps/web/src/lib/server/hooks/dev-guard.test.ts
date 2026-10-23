import { describe, expect, it, vi } from 'vitest';
import { fakeEvent } from '../../../../tests/support/event.js';
import { createDevGuard, isDevRoute } from './dev-guard.js';

describe('isDevRoute', () => {
  it.each([
    ['/dev', true],
    ['/dev/components', true],
    ['/dev/error-probe', true],
    ['/developers', false],
    ['/', false],
    [null, false],
  ] as const)('%s → %s', (id, expected) => expect(isDevRoute(id)).toBe(expected));
});

describe('createDevGuard', () => {
  const ids = ['/dev', '/dev/components', '/developers', null];

  it.each(ids)('with the catalog off, %s', async (routeId) => {
    const resolve = vi.fn(async () => new Response('page'));
    const res = await createDevGuard(undefined)({
      event: fakeEvent({ url: 'http://h/es/x', routeId }),
      resolve,
    });
    if (isDevRoute(routeId)) {
      expect(res.status).toBe(404);
      expect(await res.text()).toBe('');
      expect(resolve).not.toHaveBeenCalled();
    } else {
      expect(resolve).toHaveBeenCalledOnce();
    }
  });

  it.each(['0', 'true', 'yes', ''])('treats %j as off', async (value) => {
    const resolve = vi.fn(async () => new Response('page'));
    const res = await createDevGuard(value)({
      event: fakeEvent({ url: 'http://h/es/dev/components', routeId: '/dev/components' }),
      resolve,
    });
    expect(res.status).toBe(404);
    expect(resolve).not.toHaveBeenCalled();
  });

  it.each(ids)("with '1', resolves %s", async (routeId) => {
    const resolve = vi.fn(async () => new Response('page'));
    await createDevGuard('1')({ event: fakeEvent({ url: 'http://h/es/x', routeId }), resolve });
    expect(resolve).toHaveBeenCalledOnce();
  });
});
