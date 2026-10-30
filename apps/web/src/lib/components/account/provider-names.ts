import type { Provider } from '@torneo/protocol';
import { m } from '#lib/paraglide/messages.js';

/** The translated name of a provider: what a text or an accessible name shows, never the code. */
export const providerNames: Record<Provider, () => string> = {
  google: () => m.provider_name_google(),
  discord: () => m.provider_name_discord(),
  dev: () => m.provider_name_dev(),
};
