import { describe, expect, it, vi } from 'vitest';
import { fakeEvent } from '../../../../tests/support/event.js';
import { CSRF_ORIGIN_BODY, createOriginGuard } from './origin-guard.js';

const TRUSTED = 'https://torneo.example';
const METHODS = ['GET', 'HEAD', 'OPTIONS', 'POST', 'PUT', 'PATCH', 'DELETE'];
const ORIGINS: Record<string, string | undefined> = {
  equal: TRUSTED,
  foreign: 'https://evil.example',
  null: 'null',
  trailingSlash: `${TRUSTED}/`,
  otherScheme: 'http://torneo.example',
  uppercase: 'https://TORNEO.example',
  defaultPort: 'https://torneo.example:443',
  absent: undefined,
};
const SEC_FETCH_SITE = ['same-origin', 'same-site', 'cross-site', 'none', undefined];
const CONTENT_TYPES = ['application/x-www-form-urlencoded', 'application/json', undefined];

async function run(
  trusted: string | undefined,
  method: string,
  origin: string | undefined,
  site: string | undefined,
  contentType: string | undefined,
  extra: Record<string, string> = {},
) {
  const headers: Record<string, string> = { ...extra };
  if (origin !== undefined) headers.origin = origin;
  if (site !== undefined) headers['sec-fetch-site'] = site;
  if (contentType !== undefined) headers['content-type'] = contentType;
  const resolve = vi.fn(async () => new Response('resolved'));
  const event = fakeEvent({ url: 'http://internal:3000/es/', method, headers });
  const response = await createOriginGuard(trusted)({ event, resolve });
  return { response, resolve };
}

const cases: [string, string, string | undefined, string | undefined, boolean][] = [];
for (const method of METHODS)
  for (const [originName, origin] of Object.entries(ORIGINS))
    for (const site of SEC_FETCH_SITE)
      for (const contentType of CONTENT_TYPES) {
        const safe = ['GET', 'HEAD', 'OPTIONS'].includes(method);
        const allowed =
          safe || origin === TRUSTED || (origin === undefined && site === 'same-origin');
        cases.push([method, originName, site, contentType, allowed]);
      }

describe('createOriginGuard (RF-A23 table)', () => {
  it.each(cases)(
    '%s origin=%s site=%s type=%s → allowed=%s',
    async (method, originName, site, type, allowed) => {
      const { response, resolve } = await run(TRUSTED, method, ORIGINS[originName], site, type);
      if (allowed) {
        expect(resolve).toHaveBeenCalledOnce();
        expect(await response.text()).toBe('resolved');
      } else {
        expect(resolve).not.toHaveBeenCalled();
        expect(response.status).toBe(403);
        expect(response.headers.get('content-type')).toBe('application/json');
        expect(await response.text()).toBe(CSRF_ORIGIN_BODY);
      }
    },
  );

  it('ignores Host and X-Forwarded-Host equal to Origin', async () => {
    const { response, resolve } = await run(
      TRUSTED,
      'POST',
      'https://evil.example',
      undefined,
      'application/json',
      {
        host: 'evil.example',
        'x-forwarded-host': 'evil.example',
        'x-forwarded-proto': 'https',
      },
    );
    expect(response.status).toBe(403);
    expect(resolve).not.toHaveBeenCalled();
  });

  it.each([undefined, ''])('fails closed when the trusted origin is %j', async (trusted) => {
    for (const method of ['POST', 'PUT', 'PATCH', 'DELETE']) {
      for (const [origin, site] of [
        [undefined, 'same-origin'],
        ['', 'same-origin'],
        ['', undefined],
        [TRUSTED, 'same-origin'],
      ] as const) {
        const { response, resolve } = await run(trusted, method, origin, site, 'application/json');
        expect(response.status).toBe(403);
        expect(resolve).not.toHaveBeenCalled();
      }
    }
    const { resolve } = await run(trusted, 'GET', undefined, undefined, undefined);
    expect(resolve).toHaveBeenCalledOnce();
  });
});
