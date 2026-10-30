import type { ErrorCode } from '@torneo/protocol';
import { isErrorCode } from '#lib/features/auth/codes.js';

// The common `fetch` of the internal API (008 plan-d decision 3). Never throws, and never
// returns nor logs a response body: only the status and a validated code.

/** Kit's `event.fetch` (so `handleFetch` adds the internal headers) and INTERNAL_API_URL. */
export interface Api {
  fetch: typeof fetch;
  base: string;
}

export type ApiMethod = 'GET' | 'PUT' | 'POST';

export type ApiResult<T> =
  | { ok: true; status: number; data: T }
  | { ok: false; status: number; code: ErrorCode | undefined }
  | { ok: false; unavailable: true };

export interface ApiRequestOptions {
  body?: unknown;
  timeoutMs?: number;
}

const DEFAULT_TIMEOUT_MS = 3000;
const UNAVAILABLE = { ok: false, unavailable: true } as const;

/**
 * One request to the internal API. 2xx and 3xx are success (a form action's 303 included; the
 * redirect is never followed); a 2xx body that is not JSON counts as unavailable. Any other
 * status gives `{ status, code }`, where `code` is the `error` of the body only if the protocol
 * knows it. Network errors and the timeout give `{ unavailable: true }`.
 */
export async function apiRequest<T>(
  api: Api,
  method: ApiMethod,
  path: `/api/${string}`,
  options: ApiRequestOptions = {},
): Promise<ApiResult<T>> {
  const init: RequestInit = {
    method,
    redirect: 'manual',
    signal: AbortSignal.timeout(options.timeoutMs ?? DEFAULT_TIMEOUT_MS),
  };
  if (options.body !== undefined) {
    init.body = JSON.stringify(options.body);
    init.headers = { 'content-type': 'application/json' };
  }
  let response: Response;
  try {
    response = await api.fetch(`${api.base}${path}`, init);
  } catch {
    return UNAVAILABLE;
  }
  const { status } = response;
  if (status >= 200 && status < 400) {
    if (status !== 200) {
      await response.body?.cancel().catch(() => undefined);
      return { ok: true, status, data: undefined as T };
    }
    try {
      return { ok: true, status, data: (await response.json()) as T };
    } catch {
      return UNAVAILABLE;
    }
  }
  let code: ErrorCode | undefined;
  try {
    const body: unknown = await response.json();
    const value =
      typeof body === 'object' && body !== null && !Array.isArray(body)
        ? (body as Record<string, unknown>).error
        : undefined;
    code = isErrorCode(value) ? value : undefined;
  } catch {
    code = undefined;
  }
  return { ok: false, status, code };
}
