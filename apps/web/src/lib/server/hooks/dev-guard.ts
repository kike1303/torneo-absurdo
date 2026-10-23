import type { Handle } from '@sveltejs/kit/hooks';

export function isDevRoute(routeId: string | null): boolean {
  return routeId === '/dev' || (routeId?.startsWith('/dev/') ?? false);
}

/**
 * RF-A16: with the catalog off, every `/dev/**` route (pages, `__data.json`, endpoints and
 * actions) gets a bare 404 without `resolve`, `error(404)` or `handleError`, so nothing runs and
 * nothing is logged. Uses `route.id`, which has no locale prefix thanks to `reroute`.
 */
export function createDevGuard(enableDevCatalog: string | undefined): Handle {
  const enabled = enableDevCatalog === '1';
  return ({ event, resolve }) => {
    if (!enabled && isDevRoute(event.route.id)) return new Response(null, { status: 404 });
    return resolve(event);
  };
}
