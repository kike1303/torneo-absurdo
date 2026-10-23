import type { HandleServerError, ServerInit } from '@sveltejs/kit/hooks';
import { sequence } from '@sveltejs/kit/hooks';
import { handleError as handleServerError } from '#lib/server/hooks/handle-error.js';
import { localeRedirect } from '#lib/server/hooks/locale-redirect.js';
import { paraglide } from '#lib/server/hooks/paraglide.js';
import { installProcessHandlers, logStartup } from '#lib/server/hooks/process-handlers.js';

export const init: ServerInit = () => {
  installProcessHandlers();
  logStartup();
};

export const handle = sequence(localeRedirect, paraglide);

export const handleError: HandleServerError = handleServerError;
