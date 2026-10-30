import type { CallbackErrorCode, ErrorCode } from '@torneo/protocol';
import type { m } from '#lib/paraglide/messages.js';

/** A Paraglide message name: a key that does not exist does not compile. */
export type MessageKey = keyof typeof m;

// The text of every server code (008 RF-D6, plan-d decision 6 and deviation D1). Mapped types
// over the generated unions: a variant that `just gen` adds without its entry, or an entry the
// protocol does not have, fails `svelte-check`. `ws.*` reuse the close reason texts.

export const ERROR_TEXT = {
  'http.not_found': 'error_http_not_found',
  'http.method_not_allowed': 'error_http_method_not_allowed',
  'http.rate_limited': 'error_http_rate_limited',
  'http.invalid_body': 'error_http_invalid_body',
  'ws.origin_forbidden': 'ws_close_reason_origin_forbidden',
  'ws.capacity': 'ws_close_reason_capacity',
  'ws.invalid_message': 'ws_close_reason_invalid_message',
  'ws.echo_disabled': 'ws_close_reason_echo_disabled',
  'ws.rate_limited': 'ws_close_reason_rate_limited',
  'ws.pong_timeout': 'ws_close_reason_pong_timeout',
  'ws.idle_timeout': 'ws_close_reason_idle_timeout',
  'ws.ip_capacity': 'ws_close_reason_ip_capacity',
  'ws.user_capacity': 'ws_close_reason_user_capacity',
  'ws.forbidden_channel': 'ws_close_reason_forbidden_channel',
  'ws.too_many_subscriptions': 'ws_close_reason_too_many_subscriptions',
  'auth.csrf_origin': 'error_auth_csrf_origin',
  'auth.invalid_state': 'error_auth_invalid_state',
  'auth.banned': 'error_auth_banned',
  'auth.provider_error': 'error_auth_provider_error',
  'auth.try_again': 'error_auth_try_again',
  'auth.unauthenticated': 'error_auth_unauthenticated',
  'auth.already_linked': 'error_auth_already_linked',
  'auth.provider_already_linked': 'error_auth_provider_already_linked',
  'auth.last_provider': 'error_auth_last_provider',
  'auth.reauth_required': 'error_auth_reauth_required',
  'auth.reauth_mismatch': 'error_auth_reauth_mismatch',
  'profile.display_name_invalid': 'error_profile_display_name_invalid',
  'profile.display_name_reserved': 'error_profile_display_name_reserved',
  'profile.display_name_taken': 'error_profile_display_name_taken',
} as const satisfies { readonly [K in ErrorCode]: MessageKey };

/** The closed list of `/login?error=` (ADR-0008, amendment 2026-10-28). */
export const CALLBACK_ERROR_TEXT = {
  'auth.invalid_state': 'error_auth_invalid_state',
  'auth.banned': 'error_auth_banned',
  'auth.provider_error': 'error_auth_provider_error',
  'auth.try_again': 'error_auth_try_again',
  'auth.already_linked': 'error_auth_already_linked',
  'auth.provider_already_linked': 'error_auth_provider_already_linked',
  'auth.reauth_required': 'error_auth_reauth_required',
  'auth.reauth_mismatch': 'error_auth_reauth_mismatch',
} as const satisfies { readonly [K in CallbackErrorCode]: MessageKey };

export const GENERIC_ERROR = 'error_generic' satisfies MessageKey;

/** The key of an error text: what `AuthErrorNotice` receives, never a code. */
export type ErrorTextKey =
  | (typeof ERROR_TEXT)[ErrorCode]
  | (typeof CALLBACK_ERROR_TEXT)[CallbackErrorCode]
  | typeof GENERIC_ERROR;

/**
 * The key of `/login?error=`: `null` without the parameter, the code's key only if it is one of
 * `CallbackErrorCode` (own keys only, so `__proto__` or `toString` never match), and
 * `error_generic` for anything else. The value itself never leaves this function.
 */
export function resolveCallbackError(value: string | null): ErrorTextKey | null {
  if (value === null) return null;
  return Object.hasOwn(CALLBACK_ERROR_TEXT, value)
    ? CALLBACK_ERROR_TEXT[value as CallbackErrorCode]
    : GENERIC_ERROR;
}

/** The key of the `code` of a form action's `fail()`, with the same rule over every code. */
export function resolveApiError(code: unknown): ErrorTextKey {
  return typeof code === 'string' && Object.hasOwn(ERROR_TEXT, code)
    ? ERROR_TEXT[code as ErrorCode]
    : GENERIC_ERROR;
}
