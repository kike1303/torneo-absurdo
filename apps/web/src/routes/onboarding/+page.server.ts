import { fail, redirect } from '@sveltejs/kit';
import { safeNext } from '#lib/features/auth/next.js';
import { localeFromPathname } from '#lib/i18n/locale.js';
import { internalApi } from '#lib/server/api/context.js';
import { putDisplayName } from '#lib/server/api/mutations.js';
import { apiFailure, noSession } from '#lib/server/session-guard.js';
import type { Actions, PageServerLoad } from './$types';

// The onboarding (008 RF-D5 and deviation D3): the generated alias and a rename, or "Skip" (a
// link to `next`, never saved: it comes back on every login while the alias is generated).

export const load: PageServerLoad = ({ url, locals }) => {
  const locale = localeFromPathname(url.pathname) ?? 'en';
  const next = safeNext(url.searchParams.get('next'), locale);
  if (!locals.user) {
    if (locals.session === 'unavailable') return { state: 'unavailable' as const, next };
    const back = `/${locale}/onboarding?next=${encodeURIComponent(next)}`;
    redirect(303, `/${locale}/login?next=${encodeURIComponent(back)}`);
  }
  return { state: 'ready' as const, displayName: locals.user.display_name, next };
};

export const actions: Actions = {
  rename: async ({ request, url, locals, fetch }) => {
    const locale = localeFromPathname(url.pathname) ?? 'en';
    const form = await request.formData();
    const value = String(form.get('display_name') ?? '');
    const next = safeNext(String(form.get('next') ?? ''), locale);
    if (!locals.user) {
      if (locals.session === 'none') redirect(303, `/${locale}/login`);
      return noSession(locals.session);
    }
    const result = await putDisplayName(internalApi(fetch), value);
    if (result.ok) redirect(303, next);
    if ('status' in result && result.status === 401) redirect(303, `/${locale}/login`);
    const failure = apiFailure(result);
    return fail(failure.status, { ...failure.data, value });
  },
};
