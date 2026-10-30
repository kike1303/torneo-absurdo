import { building } from '$app/env';
import { defineEnvVars } from '@sveltejs/kit/env';
import {
  validateEnableDevCatalog,
  validateInternalApiSecret,
  validateInternalApiUrl,
  validatePublicOrigin,
} from '#lib/env/validators.js';

// Private and dynamic (read when the app starts); read them from `$app/env/private` (RF-A2).
// The checks that compare two variables run in `init` of hooks.server.ts (008 decision 1).
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
  INTERNAL_API_URL: {
    description:
      'Origin of the server for internal requests (ADR-0008 D5), different from PUBLIC_ORIGIN. Required at runtime; production uses http://torneo-server:8080.',
    schema: (value) => validateInternalApiUrl(value, building),
  },
  INTERNAL_API_SECRET: {
    description:
      'Shared secret sent in X-Internal-Secret, only to INTERNAL_API_URL (RF-D1). base64url of at least 32 bytes; never the development value outside loopback.',
    schema: (value) => validateInternalApiSecret(value, building),
  },
});
