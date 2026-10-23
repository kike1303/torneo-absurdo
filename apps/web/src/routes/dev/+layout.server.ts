import { error } from '@sveltejs/kit';
import { ENABLE_DEV_CATALOG } from '$app/env/private';
import type { LayoutServerLoad } from './$types';

// Second layer of the RF-A16 guard (the first one is `devGuard` in `handle`).
export const load: LayoutServerLoad = () => {
  if (ENABLE_DEV_CATALOG !== '1') error(404);
};
