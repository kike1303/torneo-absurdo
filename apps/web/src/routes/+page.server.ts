import { ENABLE_DEV_CATALOG } from '$app/env/private';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = () => ({ catalog: ENABLE_DEV_CATALOG === '1' });
