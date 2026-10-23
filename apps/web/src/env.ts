import { defineEnvVars } from '@sveltejs/kit/env';
import { building } from '$app/env';

// Step 2 skeleton: full validators arrive in step 6 (src/lib/env/validators.ts).
export const variables = defineEnvVars({
  PUBLIC_ORIGIN: {
    description: 'Exact browser origin of the app (http(s)://host[:port]). Required at runtime.',
    schema: (value) => {
      if (value === undefined || value === '') {
        if (building) return undefined;
        throw new Error('PUBLIC_ORIGIN is required at runtime');
      }
      return value;
    },
  },
  ENABLE_DEV_CATALOG: {
    description: "Enables /dev/** only when exactly '1'. Never set in production.",
    schema: (value) => (value === '1' ? '1' : undefined),
  },
});
