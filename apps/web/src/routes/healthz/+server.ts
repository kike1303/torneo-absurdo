import type { RequestHandler } from './$types';

// RF-A18: no API call, no locale redirect, no logs.
export const GET: RequestHandler = () =>
  new Response('ok', {
    headers: { 'content-type': 'text/plain; charset=utf-8', 'cache-control': 'no-store' },
  });
