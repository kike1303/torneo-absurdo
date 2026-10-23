import { ENABLE_DEV_CATALOG, PUBLIC_ORIGIN } from '$app/env/private';
import type { HandleServerError, ServerInit } from '@sveltejs/kit/hooks';
import { sequence } from '@sveltejs/kit/hooks';
import { createDevGuard } from '#lib/server/hooks/dev-guard.js';
import { handleError as handleServerError } from '#lib/server/hooks/handle-error.js';
import { localeRedirect } from '#lib/server/hooks/locale-redirect.js';
import { createOriginGuard } from '#lib/server/hooks/origin-guard.js';
import { paraglide } from '#lib/server/hooks/paraglide.js';
import { installProcessHandlers, logStartup } from '#lib/server/hooks/process-handlers.js';
import { securityHeaders } from '#lib/server/hooks/security-headers.js';
import { createThemeHandle } from '#lib/server/hooks/theme.js';

export const init: ServerInit = () => {
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
