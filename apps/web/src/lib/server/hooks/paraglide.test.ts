import { describe, expect, it, vi } from 'vitest';
import { fakeEvent } from '../../../../tests/support/event.js';

vi.mock('#lib/paraglide/server.js', () => ({
  paraglideMiddleware: vi.fn(
    async () =>
      new Response(null, {
        status: 307,
        headers: { Location: 'http://evil.example/es/x?y=1', Vary: 'Accept' },
      }),
  ),
}));

const { paraglide } = await import('./paraglide.js');
const { paraglideMiddleware } = await import('#lib/paraglide/server.js');

describe('paraglide handle', () => {
  it("rewrites the middleware's absolute redirect to pathname + search", async () => {
    const resolve = vi.fn(async () => new Response('ok'));
    const event = fakeEvent({
      url: 'http://evil.example/es',
      headers: { 'sec-fetch-dest': 'document' },
    });
    const res = await paraglide({ event, resolve });
    expect(res.status).toBe(307);
    expect(res.headers.get('location')).toBe('/es/x?y=1');
  });

  it('skips the middleware on excluded paths', async () => {
    vi.mocked(paraglideMiddleware).mockClear();
    const resolve = vi.fn(async () => new Response('ok'));
    const res = await paraglide({ event: fakeEvent({ url: 'http://h/healthz' }), resolve });
    expect(await res.text()).toBe('ok');
    expect(paraglideMiddleware).not.toHaveBeenCalled();
  });
});
