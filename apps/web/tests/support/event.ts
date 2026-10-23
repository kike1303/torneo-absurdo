import type { RequestEvent } from '@sveltejs/kit';

export interface FakeEventInit {
  url: string;
  method?: string;
  headers?: Record<string, string>;
  cookies?: Record<string, string>;
  routeId?: string | null;
}

/** Minimal `RequestEvent` for unit tests of `handle` pieces. Only the fields our hooks read. */
export function fakeEvent(init: FakeEventInit): RequestEvent {
  const url = new URL(init.url);
  const request = new Request(url, {
    method: init.method ?? 'GET',
    headers: init.headers ?? {},
  });
  const cookies = init.cookies ?? {};
  return {
    url,
    request,
    route: { id: init.routeId ?? null },
    locals: {},
    cookies: {
      get: (name: string) => cookies[name],
      getAll: () => Object.entries(cookies).map(([name, value]) => ({ name, value })),
    },
  } as unknown as RequestEvent;
}
