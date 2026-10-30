import type { RequestEvent } from '@sveltejs/kit';

export interface FakeEventInit {
  url: string;
  method?: string;
  headers?: Record<string, string>;
  cookies?: Record<string, string>;
  routeId?: string | null;
  /** Initial `locals` (008: `user` and `session`). */
  locals?: Partial<App.Locals>;
  /** `event.fetch` (008: the internal API). */
  fetch?: typeof globalThis.fetch;
  /** A form body, sent as `application/x-www-form-urlencoded` (form actions). */
  form?: Record<string, string>;
}

export interface WrittenCookie {
  name: string;
  value: string;
  options: Record<string, unknown>;
}

const WRITTEN = Symbol('written cookies');

/** Minimal `RequestEvent` for unit tests of `handle` pieces. Only the fields our hooks read. */
export function fakeEvent(init: FakeEventInit): RequestEvent {
  const url = new URL(init.url);
  const headers: Record<string, string> = { ...init.headers };
  let body: string | undefined;
  if (init.form) {
    body = new URLSearchParams(init.form).toString();
    headers['content-type'] = 'application/x-www-form-urlencoded';
  }
  const request = new Request(url, {
    method: init.method ?? (init.form ? 'POST' : 'GET'),
    headers,
    body,
  });
  const cookies = { ...init.cookies };
  const written: WrittenCookie[] = [];
  return {
    url,
    request,
    route: { id: init.routeId ?? null },
    locals: { ...init.locals },
    fetch:
      init.fetch ??
      (() => {
        throw new Error('fetch not faked');
      }),
    getClientAddress: () => '203.0.113.5',
    cookies: {
      [WRITTEN]: written,
      get: (name: string) => cookies[name],
      getAll: () => Object.entries(cookies).map(([name, value]) => ({ name, value })),
      set: (name: string, value: string, options: Record<string, unknown>) => {
        cookies[name] = value;
        written.push({ name, value, options });
      },
      delete: (name: string, options: Record<string, unknown>) => {
        delete cookies[name];
        written.push({ name, value: '', options: { ...options, maxAge: 0 } });
      },
    },
  } as unknown as RequestEvent;
}

/** The `cookies.set` and `cookies.delete` calls of a fake event, in order. */
export function writtenCookies(event: RequestEvent): WrittenCookie[] {
  return (event.cookies as unknown as Record<symbol, WrittenCookie[]>)[WRITTEN] ?? [];
}

/** Runs a `load` or an action and returns the redirect it throws (Kit's `redirect()`). */
export async function redirectOf(
  run: () => unknown,
): Promise<{ status: number; location: string }> {
  try {
    await run();
  } catch (e) {
    const r = e as { status?: unknown; location?: unknown };
    if (typeof r.status === 'number' && typeof r.location === 'string') {
      return { status: r.status, location: r.location };
    }
    throw e;
  }
  throw new Error('expected a redirect');
}
