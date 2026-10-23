import type { Reroute } from '@sveltejs/kit/hooks';
import { deLocalizeUrl } from '#lib/paraglide/runtime.js';

// `/es/dev/components` → `/dev/components`, so `route.id` never carries the locale prefix.
export const reroute: Reroute = ({ url }) => deLocalizeUrl(url).pathname;
