import type { HealthResponse } from '@torneo/protocol';

// Server status for /dev/status (RF-B7). Client-side only: in 001 the SSR never calls the API.

export type HealthState =
  { kind: 'ok'; version: string } | { kind: 'db_unavailable' } | { kind: 'network_error' };

/** Paraglide keys of each state. */
export type HealthMessageKey =
  'dev_status_server_ok' | 'dev_status_server_db_unavailable' | 'dev_status_server_network_error';

/** Type guard: an unexpected shape counts as a network error. */
export function isHealthResponse(value: unknown): value is HealthResponse {
  if (typeof value !== 'object' || value === null) return false;
  const body = value as Record<string, unknown>;
  if (body.status === 'ok') return typeof body.version === 'string';
  return body.status === 'db_unavailable';
}

export async function fetchHealth(fetchFn: typeof fetch = fetch): Promise<HealthState> {
  try {
    const response = await fetchFn('/api/health', {
      headers: { accept: 'application/json' },
      cache: 'no-store',
    });
    const body: unknown = await response.json();
    if (!isHealthResponse(body)) return { kind: 'network_error' };
    return body.status === 'ok'
      ? { kind: 'ok', version: body.version }
      : { kind: 'db_unavailable' };
  } catch {
    return { kind: 'network_error' };
  }
}

export function healthMessageKey(state: HealthState): HealthMessageKey {
  switch (state.kind) {
    case 'ok':
      return 'dev_status_server_ok';
    case 'db_unavailable':
      return 'dev_status_server_db_unavailable';
    case 'network_error':
      return 'dev_status_server_network_error';
  }
}
