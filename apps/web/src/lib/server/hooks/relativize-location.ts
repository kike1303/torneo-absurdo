import { stripLeadingSlashes } from '#lib/i18n/locale.js';

/**
 * Rewrites the `Location` of a 3xx response to `pathname + search`, relative to the same origin.
 * `paraglideMiddleware` redirects to an absolute URL built from the request's `Host`; RF-A3 wants
 * every locale redirect to be relative (plan-a decision 2).
 */
export function relativizeLocation(response: Response): Response {
  if (response.status < 300 || response.status > 399) return response;
  const location = response.headers.get('location');
  if (location === null) return response;
  const relative = toRelative(location);
  try {
    response.headers.set('location', relative);
    return response;
  } catch (error) {
    if (!(error instanceof TypeError)) throw error;
  }
  const copy = new Response(response.body, response);
  copy.headers.set('location', relative);
  return copy;
}

function toRelative(location: string): string {
  let pathname: string;
  let search = '';
  if (URL.canParse(location)) {
    const parsed = new URL(location);
    pathname = parsed.pathname;
    search = parsed.search;
  } else {
    const queryAt = location.search(/[?#]/);
    pathname = queryAt === -1 ? location : location.slice(0, queryAt);
    if (queryAt !== -1 && location[queryAt] === '?') {
      const hashAt = location.indexOf('#', queryAt);
      search = hashAt === -1 ? location.slice(queryAt) : location.slice(queryAt, hashAt);
    }
  }
  return `/${stripLeadingSlashes(pathname)}${search}`;
}
