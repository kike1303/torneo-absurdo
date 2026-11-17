import { fail, redirect } from '@sveltejs/kit';
import type { ErrorCode } from '@torneo/protocol';
import { parseBuyForm, parseItemParam } from '#lib/features/shop/shop.js';
import { localeFromPathname } from '#lib/i18n/locale.js';
import { internalApi } from '#lib/server/api/context.js';
import { postShopBuy } from '#lib/server/api/mutations.js';
import { getBotarga, getShop } from '#lib/server/api/reads.js';
import { apiFailure, noSession } from '#lib/server/session-guard.js';
import type { Actions, PageServerLoad } from './$types';

// The shop (024): only with a session. The item to try on is in the URL (`?item=<kind>:<id>`), so
// picking one works without JavaScript; the purchase is a form action with the price the page
// showed and a key new on each load, so a reload of the POST never buys twice. The server decides
// every rule. A failure of the API is a 200 with its own state, never a 500.

export const load: PageServerLoad = async ({ fetch, url, locals, setHeaders }) => {
  setHeaders({ 'cache-control': 'private, no-store' });
  const locale = localeFromPathname(url.pathname) ?? 'en';
  if (!locals.user) {
    if (locals.session === 'unavailable') return { state: 'unavailable' as const };
    redirect(303, `/${locale}/login?next=${encodeURIComponent(`/${locale}/shop`)}`);
  }
  const api = internalApi(fetch);
  const [shop, botarga] = await Promise.all([getShop(api), getBotarga(api)]);
  if (!botarga.ok && 'status' in botarga && botarga.status === 404) {
    return { state: 'none' as const };
  }
  if (!shop.ok || !botarga.ok) return { state: 'error' as const };
  const picked = parseItemParam(url.searchParams.get('item'));
  return {
    state: 'ready' as const,
    shop: shop.data,
    botarga: botarga.data,
    // Only an item of the shop is tried on.
    picked:
      picked !== null &&
      shop.data.items.some((item) => item.kind === picked.kind && item.id === picked.id)
        ? picked
        : null,
    clientKey: crypto.randomUUID(),
  };
};

export const actions: Actions = {
  buy: async ({ request, url, locals, fetch }) => {
    if (!locals.user) {
      if (locals.session === 'none') {
        const locale = localeFromPathname(url.pathname) ?? 'en';
        redirect(303, `/${locale}/login?next=${encodeURIComponent(`/${locale}/shop`)}`);
      }
      return noSession(locals.session);
    }
    const body = parseBuyForm(await request.formData());
    if (body === null) return fail(422, { code: 'shop.invalid_request' as ErrorCode });
    const result = await postShopBuy(internalApi(fetch), body);
    if (!result.ok) return apiFailure(result);
    return { bought: { kind: result.data.kind, id: result.data.item_id } };
  },
};
