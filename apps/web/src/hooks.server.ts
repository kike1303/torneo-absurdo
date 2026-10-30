import {
  ENABLE_DEV_CATALOG,
  INTERNAL_API_SECRET,
  INTERNAL_API_URL,
  PUBLIC_ORIGIN,
} from '$app/env/private';
import type { HandleFetch, HandleServerError, ServerInit } from '@sveltejs/kit/hooks';
import { sequence } from '@sveltejs/kit/hooks';
import { checkInternalApiEnv } from '#lib/env/validators.js';
import { createHandleFetch } from '#lib/server/hooks/handle-fetch.js';
import { createDevGuard } from '#lib/server/hooks/dev-guard.js';
import { handleError as handleServerError } from '#lib/server/hooks/handle-error.js';
import { localeRedirect } from '#lib/server/hooks/locale-redirect.js';
import { createOriginGuard } from '#lib/server/hooks/origin-guard.js';
import { paraglide } from '#lib/server/hooks/paraglide.js';
import { installProcessHandlers, logStartup } from '#lib/server/hooks/process-handlers.js';
import { securityHeaders } from '#lib/server/hooks/security-headers.js';
import { createThemeHandle } from '#lib/server/hooks/theme.js';

export const init: ServerInit = () => {
  // Before anything listens: a misconfiguration stops the start naming only the variable.
  checkInternalApiEnv({
    publicOrigin: PUBLIC_ORIGIN,
    internalApiUrl: INTERNAL_API_URL,
    internalApiSecret: INTERNAL_API_SECRET,
  });
  installProcessHandlers();
  logStartup();
};

// Order (plan-a decision 3): headers wrap everything; the Origin guard is the first that can
// answer; then the locale redirect, Paraglide, the /dev/** guard and the theme.
export const handle = sequence(
  securityHeaders,
  createOriginGuard(PUBLIC_ORIGIN),
  localeRedirect,
  paraglide,
  createDevGuard(ENABLE_DEV_CATALOG),
  createThemeHandle(ENABLE_DEV_CATALOG),
);

export const handleError: HandleServerError = handleServerError;

// Internal headers only towards INTERNAL_API_URL; every other fetch leaves without them (RF-D1).
export const handleFetch: HandleFetch = createHandleFetch({
  internalApiUrl: INTERNAL_API_URL ?? '',
  secret: INTERNAL_API_SECRET ?? '',
  publicOrigin: PUBLIC_ORIGIN ?? '',
});
