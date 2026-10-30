import type { CallbackErrorCode, ErrorCode } from '@torneo/protocol';

// `@torneo/protocol` exports only types (`export type *`), so the closed lists exist at run time
// here, as mapped types over the generated unions: a variant that `just gen` adds without its
// entry, or an entry the protocol does not have, fails `svelte-check` (008 plan-d decision 6).

const ERROR_CODES: { readonly [K in ErrorCode]: true } = {
  'http.not_found': true,
  'http.method_not_allowed': true,
  'http.rate_limited': true,
  'http.invalid_body': true,
  'ws.origin_forbidden': true,
  'ws.capacity': true,
  'ws.invalid_message': true,
  'ws.echo_disabled': true,
  'ws.rate_limited': true,
  'ws.pong_timeout': true,
  'ws.idle_timeout': true,
  'ws.ip_capacity': true,
  'ws.user_capacity': true,
  'ws.forbidden_channel': true,
  'ws.too_many_subscriptions': true,
  'auth.csrf_origin': true,
  'auth.invalid_state': true,
  'auth.banned': true,
  'auth.provider_error': true,
  'auth.try_again': true,
  'auth.unauthenticated': true,
  'auth.already_linked': true,
  'auth.provider_already_linked': true,
  'auth.last_provider': true,
  'auth.reauth_required': true,
  'auth.reauth_mismatch': true,
  'profile.display_name_invalid': true,
  'profile.display_name_reserved': true,
  'profile.display_name_taken': true,
};

const CALLBACK_ERROR_CODES: { readonly [K in CallbackErrorCode]: true } = {
  'auth.invalid_state': true,
  'auth.banned': true,
  'auth.provider_error': true,
  'auth.try_again': true,
  'auth.already_linked': true,
  'auth.provider_already_linked': true,
  'auth.reauth_required': true,
  'auth.reauth_mismatch': true,
};

/** Only own keys: `__proto__`, `constructor` or `toString` are never codes. */
export function isErrorCode(value: unknown): value is ErrorCode {
  return typeof value === 'string' && Object.hasOwn(ERROR_CODES, value);
}

export function isCallbackErrorCode(value: unknown): value is CallbackErrorCode {
  return typeof value === 'string' && Object.hasOwn(CALLBACK_ERROR_CODES, value);
}
