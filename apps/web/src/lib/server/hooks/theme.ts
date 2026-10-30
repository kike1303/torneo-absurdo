import type { Handle } from '@sveltejs/kit/hooks';
import {
  THEME_COOKIE,
  resolveTheme,
  themeMeta,
  type ThemeChoice,
} from '#lib/theme/resolve-theme.js';

/**
 * Replaces `%theme%` and `%theme.meta%` only in the part of the document before `</head>` (which
 * includes `<html>`) and only their first occurrence. Chunks after `</head>` pass untouched, so a
 * marker in future user text in the body is never expanded (plan-a decision 3, handle 6).
 */
export function createHeadTransformer(choice: ThemeChoice): (html: string) => string {
  let headDone = false;
  let themeDone = false;
  let metaDone = false;
  return (html) => {
    if (headDone) return html;
    const end = html.indexOf('</head>');
    let head = end === -1 ? html : html.slice(0, end);
    const tail = end === -1 ? '' : html.slice(end);
    if (!metaDone && head.includes('%theme.meta%')) {
      head = head.replace('%theme.meta%', themeMeta(choice));
      metaDone = true;
    }
    if (!themeDone && head.includes('%theme%')) {
      head = head.replace('%theme%', choice);
      themeDone = true;
    }
    if (end !== -1) headDone = true;
    return head + tail;
  };
}

/**
 * RF-A6, RF-A9 and 008 RF-D4: resolves the theme (the profile's first, with a session), stores
 * it in `locals` and fills the head markers.
 */
export function createThemeHandle(enableDevCatalog: string | undefined): Handle {
  return ({ event, resolve }) => {
    const choice = resolveTheme({
      profile: event.locals.user?.theme ?? null,
      cookie: event.cookies.get(THEME_COOKIE),
      override: event.url.searchParams.get('theme'),
      enableDevCatalog,
      routeId: event.route.id,
    });
    event.locals.theme = choice;
    const transform = createHeadTransformer(choice);
    return resolve(event, { transformPageChunk: ({ html }) => transform(html) });
  };
}
