import { expect, test } from '@playwright/test';
import { ALLOWED_FIELDS, CANARIES, linesSince, readLines } from './support/logs.js';
import { raw } from './support/raw-http.js';

// The web directly: the edge drops the canary X-Forwarded-For (008 plan-d decision 11).
const PORT = 4175;
const CANARY_HEADERS = {
  cookie: `session=${CANARIES.cookie}; theme=dark`,
  authorization: `Bearer ${CANARIES.authorization}`,
  'x-internal-secret': CANARIES.secret,
  'x-client-ip': CANARIES.clientIp,
  'x-forwarded-for': CANARIES.forwardedFor,
};

test('A5 / RF-A19: no client data in the logs or in the error page', async () => {
  const before = readLines('catalog').length;

  const home = await raw(PORT, '/es/', { headers: CANARY_HEADERS });
  expect(home.status).toBe(200);

  const probe = await raw(PORT, `/es/dev/error-probe?q=${CANARIES.requestQuery}`, {
    headers: CANARY_HEADERS,
  });
  expect(probe.status).toBe(500);
  for (const canary of Object.values(CANARIES)) expect(probe.body).not.toContain(canary);
  const pageErrorId = /data-testid="error-id"[^>]*>[^<]*?([0-9a-f-]{36})/.exec(probe.body)?.[1];
  expect(pageErrorId).toBeDefined();

  const lines = await linesSince('catalog', before);
  expect(lines).toHaveLength(1);
  for (const line of lines) {
    for (const canary of Object.values(CANARIES)) expect(line).not.toContain(canary);
  }
  const entry = JSON.parse(lines[0]!) as Record<string, unknown>;
  expect(Object.keys(entry).every((key) => ALLOWED_FIELDS.has(key))).toBe(true);
  expect(entry.error_id).toBe(pageErrorId);
  expect(entry.route_id).toBe('/dev/error-probe');
  expect(entry.status).toBe(500);
  expect(entry).not.toHaveProperty('error_message');
  expect(entry.error_name).toBe('other');
  expect(entry.stack).toMatch(/^ +at /);
});

test('RF-A19: a 404 writes nothing and shows no error id', async () => {
  const before = readLines('catalog').length;
  const res = await raw(PORT, '/es/no-existe', { headers: CANARY_HEADERS });
  expect(res.status).toBe(404);
  expect(res.body).not.toContain('data-testid="error-id"');
  expect(await linesSince('catalog', before)).toEqual([]);
});
