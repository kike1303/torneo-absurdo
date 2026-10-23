import type { LayoutServerLoad } from './$types';

// The theme the SSR rendered with, so the client runa starts in sync (RF-A8).
export const load: LayoutServerLoad = ({ locals }) => ({ theme: locals.theme });
