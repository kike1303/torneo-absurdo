import { expect, test } from '@playwright/test';
import { linesSince, readLines } from './support/logs.js';
import { header, raw, type RawResponse } from './support/raw-http.js';

const CATALOG = 4173;
const NO_CATALOG = 4174;

function expectSecurityHeaders(res: RawResponse, label: string): void {
  expect(header(res, 'x-content-type-options'), label).toBe('nosniff');
  expect(header(res, 'referrer-policy'), label).toBe('strict-origin-when-cross-origin');
  expect(header(res, 'strict-transport-security'), label).toBe('max-age=31536000');
}

test('RF-A10: security headers on 200, 307, 403 and 404, and a CSP with nonce', async () => {
  const ok = await raw(CATALOG, '/es/');
  expect(ok.status).toBe(200);
  expectSecurityHeaders(ok, '200');
  const csp = header(ok, 'content-security-policy') ?? '';
  expect(csp).toContain("script-src 'self' 'nonce-");
  expect(csp).toContain("frame-ancestors 'none'");
  expect(csp).not.toMatch(/script-src[^;]*'(unsafe-inline|unsafe-eval|strict-dynamic)'/);

  const redirect = await raw(CATALOG, '/');
  expect(redirect.status).toBe(307);
  expectSecurityHeaders(redirect, '307');

  const forbidden = await raw(CATALOG, '/es/', {
    method: 'POST',
    headers: { 'content-type': 'application/json', origin: 'https://evil.example' },
    body: '{}',
  });
  expect(forbidden.status).toBe(403);
  expectSecurityHeaders(forbidden, '403');

  const missing = await raw(CATALOG, '/es/no-existe');
  expect(missing.status).toBe(404);
  expectSecurityHeaders(missing, '404');

  const guarded = await raw(NO_CATALOG, '/es/dev/components');
  expect(guarded.status).toBe(404);
  expectSecurityHeaders(guarded, 'dev guard 404');
});

test('RF-A18: /healthz answers 200 without Location and without logging', async () => {
  const before = readLines('catalog').length;
  const res = await raw(CATALOG, '/healthz');
  expect(res.status).toBe(200);
  expect(header(res, 'location')).toBeUndefined();
  expect(await linesSince('catalog', before)).toEqual([]);
});

test('RF-A23: Origin guard rejects foreign and missing origins, without logging', async () => {
  const before = readLines('catalog').length;
  const foreign = await raw(CATALOG, '/es/', {
    method: 'POST',
    headers: { 'content-type': 'application/json', origin: 'https://evil.example' },
    body: '{}',
  });
  expect(foreign.status).toBe(403);
  expect(foreign.body).toBe('{"error":"auth.csrf_origin"}');
  expect(header(foreign, 'content-type')).toBe('application/json');

  const noOrigin = await raw(CATALOG, '/es/', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: '{}',
  });
  expect(noOrigin.status).toBe(403);
  expect(noOrigin.body).toBe('{"error":"auth.csrf_origin"}');

  const healthz = await raw(CATALOG, '/healthz', {
    method: 'POST',
    headers: { 'content-type': 'application/json', origin: `http://localhost:${CATALOG}` },
    body: '{}',
  });
  expect(healthz.status).toBe(405);

  expect(await linesSince('catalog', before)).toEqual([]);
});

test('RF-A16 / test 6: without ENABLE_DEV_CATALOG every /dev/** route is 404 and nothing is logged', async () => {
  const before = readLines('no-catalog').length;
  for (const path of [
    '/es/dev/components',
    '/en/dev/components',
    '/es/dev/error-probe',
    '/es/dev/components/__data.json',
    // Does not exist in phase A (arrives in B): a 404 here is not guard coverage.
    '/es/dev/status',
  ]) {
    const res = await raw(NO_CATALOG, path);
    expect(res.status, path).toBe(404);
  }
  const post = await raw(NO_CATALOG, '/es/dev/error-probe', {
    method: 'POST',
    headers: { 'content-type': 'application/json', origin: `http://localhost:${NO_CATALOG}` },
    body: '{}',
  });
  expect(post.status).toBe(404);
  expect(post.body).toBe('');
  expect(await linesSince('no-catalog', before)).toEqual([]);
});

test('test 6: the catalog exists on the instance with the variable', async () => {
  expect((await raw(CATALOG, '/es/dev/components')).status).toBe(200);
});
