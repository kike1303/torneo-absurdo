import { sequence } from '@sveltejs/kit/hooks';
import { localeRedirect } from '#lib/server/hooks/locale-redirect.js';
import { paraglide } from '#lib/server/hooks/paraglide.js';

export const handle = sequence(localeRedirect, paraglide);
