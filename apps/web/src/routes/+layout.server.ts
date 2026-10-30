import type { LayoutServerLoad } from './$types';

// The theme the SSR rendered with, so the client runa starts in sync (RF-A8), and for the header
// only the alias, the balance and the session state (008 plan-d decision 4).
export const load: LayoutServerLoad = ({ locals }) => ({
  theme: locals.theme,
  session: locals.session,
  user: locals.user
    ? { display_name: locals.user.display_name, balance: locals.user.balance }
    : null,
});
