import { INTERNAL_API_URL } from '$app/env/private';
import type { Api } from './client.js';

/**
 * The internal API through Kit's `event.fetch`, so `handleFetch` adds the session cookie and the
 * internal headers (008 RF-D1). INTERNAL_API_URL is required at runtime (`src/env.ts`).
 */
export function internalApi(fetch: typeof globalThis.fetch): Api {
  return { fetch, base: INTERNAL_API_URL ?? '' };
}
