import { expect, test } from '@playwright/test';
import { linesSince, readLines } from './support/logs.js';
import { header, raw, type RawResponse } from './support/raw-http.js';

const CATALOG = 4173;
const NO_CATALOG = 4174;

function expectSecurityHeaders(res: RawResponse, label: string): void {
  expect(header(res, 'x-content-type-options'), label).toBe('nosniff');
  expect(header(res, 'referrer-policy'), label).toBe('strict-origin-when-cross-origin');
  expect(header(res, 'strict-transport-security'), label).toBe('max-age=31536000');
  expect(header(res, 'cross-origin-opener-policy'), label).toBe('same-origin');
  expect(header(res, 'permissions-policy'), label).toBe(
    'camera=(), microphone=(), geolocation=(), payment=(), usb=(), display-capture=()',
  );
}

// ADR-0012 B1 as the browser receives it, with Kit's per-request nonce as `'nonce-*'`. The
// static check (scripts/check-kit-config.mjs) can only read vite.config.ts; this compares what Kit
// actually emits, so a policy assembled some other way still fails (security-reviewer on #50).
// A B4 extension changes this table in its own PR (B4.6 below).
const CSP_B1: Record<string, string[]> = {
  'default-src': ["'self'"],
  'script-src': ["'self'", "'nonce-*'"],
  'style-src': ["'self'", "'unsafe-inline'"],
  'img-src': ["'self'", 'data:', 'blob:'],
  'font-src': ["'self'"],
  'connect-src': ["'self'"],
  'object-src': ["'none'"],
  'base-uri': ["'self'"],
  // B4.6 (2026-10-30 amendment): the authorize endpoints of the two providers, with their path.
  'form-action': [
    "'self'",
    'https://accounts.google.com/o/oauth2/v2/auth',
    'https://discord.com/oauth2/authorize',
  ],
  'frame-ancestors': ["'none'"],
};

/** Directive → sorted sources; a repeated directive is kept as its own key so it fails. */
function effectiveCsp(value: string): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  for (const part of value.split(';')) {
    const [name, ...sources] = part.trim().split(/\s+/);
    if (!name) continue;
    const key = name in out ? `${name} (repeated)` : name;
    out[key] = sources.map((s) => s.replace(/^'nonce-[A-Za-z0-9+/_=-]+'$/, "'nonce-*'")).sort();
  }
  return out;
}

test('RF-A10: security headers on 200, 307, 403 and 404, and a CSP with nonce', async () => {
  const ok = await raw(CATALOG, '/es/');
  expect(ok.status).toBe(200);
  expectSecurityHeaders(ok, '200');
  const b1 = Object.fromEntries(Object.entries(CSP_B1).map(([k, v]) => [k, [...v].sort()]));
  expect(effectiveCsp(header(ok, 'content-security-policy') ?? '')).toEqual(b1);

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

// TA-10: pins the handle order (plan-a decision 3). The Origin guard answers before the locale
// redirect (307) and before the /dev/** guard (404); RF-A23 forbids either answering first.
test('RF-A23: POST / with a foreign Origin is 403, not the locale 307', async () => {
  const res = await raw(CATALOG, '/', {
    method: 'POST',
    headers: { 'content-type': 'application/json', origin: 'https://evil.example' },
    body: '{}',
  });
  expect(res.status).toBe(403);
  expect(res.body).toBe('{"error":"auth.csrf_origin"}');
  expect(header(res, 'location')).toBeUndefined();
});

test('RF-A23: POST /es/dev/components with a foreign Origin is 403, not the dev guard 404', async () => {
  const res = await raw(NO_CATALOG, '/es/dev/components', {
    method: 'POST',
    headers: { 'content-type': 'application/json', origin: 'https://evil.example' },
    body: '{}',
  });
  expect(res.status).toBe(403);
  expect(res.body).toBe('{"error":"auth.csrf_origin"}');
});

test('RF-A16 / test 6: without ENABLE_DEV_CATALOG every /dev/** route is 404 and nothing is logged', async () => {
  const before = readLines('no-catalog').length;
  for (const path of [
    '/es/dev/components',
    '/en/dev/components',
    '/es/dev/error-probe',
    '/es/dev/components/__data.json',
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
