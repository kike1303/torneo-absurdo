import { building } from '$app/env';
import { defineEnvVars } from '@sveltejs/kit/env';
import { validateEnableDevCatalog, validatePublicOrigin } from '#lib/env/validators.js';

// Private and dynamic (read when the app starts); read them from `$app/env/private` (RF-A2).
export const variables = defineEnvVars({
  PUBLIC_ORIGIN: {
    description:
      'Exact browser origin of the app (http(s)://host[:port]). Required at runtime; the Origin guard trusts only this value (RF-A23).',
    schema: (value) => validatePublicOrigin(value, building),
  },
  ENABLE_DEV_CATALOG: {
    description: "Enables /dev/** only when exactly '1' (RF-A16). Never set in production.",
    schema: validateEnableDevCatalog,
  },
});
