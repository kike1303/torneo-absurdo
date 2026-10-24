import { describe, expect, it, vi } from 'vitest';
import { fetchHealth, healthMessageKey, isHealthResponse } from './health.js';

const respond = (status: number, body: unknown) =>
  vi.fn(async () => new Response(JSON.stringify(body), { status }));

describe('fetchHealth (RF-B7)', () => {
  it('200 ok → ok with the version', async () => {
    const fetchFn = respond(200, { status: 'ok', version: 'dev' });
    expect(await fetchHealth(fetchFn)).toEqual({ kind: 'ok', version: 'dev' });
    expect(fetchFn).toHaveBeenCalledWith('/api/health', expect.anything());
  });

  it('503 db_unavailable → db_unavailable', async () => {
    expect(await fetchHealth(respond(503, { status: 'db_unavailable' }))).toEqual({
      kind: 'db_unavailable',
    });
  });

  it.each([
    [{ status: 'ok' }],
    [{ status: 'weird' }],
    [{ error: 'http.not_found' }],
    [null],
    ['ok'],
  ])('an unexpected shape (%j) → network_error', async (body) => {
    expect(await fetchHealth(respond(200, body))).toEqual({ kind: 'network_error' });
  });

  it('a body that is not JSON → network_error', async () => {
    const fetchFn = vi.fn(async () => new Response('<html>', { status: 502 }));
    expect(await fetchHealth(fetchFn)).toEqual({ kind: 'network_error' });
  });

  it('a rejected fetch → network_error', async () => {
    const fetchFn = vi.fn(async () => {
      throw new TypeError('Failed to fetch');
    });
    expect(await fetchHealth(fetchFn)).toEqual({ kind: 'network_error' });
  });
});

describe('isHealthResponse', () => {
  it('accepts both shapes of HealthResponse', () => {
    expect(isHealthResponse({ status: 'ok', version: 'v1.2.3' })).toBe(true);
    expect(isHealthResponse({ status: 'db_unavailable' })).toBe(true);
    expect(isHealthResponse({ status: 'ok', version: 1 })).toBe(false);
  });
});

describe('healthMessageKey', () => {
  it('maps every state to a key', () => {
    expect(healthMessageKey({ kind: 'ok', version: 'dev' })).toBe('dev_status_server_ok');
    expect(healthMessageKey({ kind: 'db_unavailable' })).toBe('dev_status_server_db_unavailable');
    expect(healthMessageKey({ kind: 'network_error' })).toBe('dev_status_server_network_error');
  });
});
