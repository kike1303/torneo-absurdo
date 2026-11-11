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
  'auth.sessions_not_closed': 'error_auth_sessions_not_closed',
  'profile.display_name_invalid': 'error_profile_display_name_invalid',
  'profile.display_name_reserved': 'error_profile_display_name_reserved',
  'profile.display_name_taken': 'error_profile_display_name_taken',
  'fight.not_found': 'error_fight_not_found',
  'fight.unavailable': 'error_fight_unavailable',
  'bet.invalid_request': 'error_bet_invalid_request',
  'bet.key_reused': 'error_bet_key_reused',
  'bet.window_closed': 'error_bet_window_closed',
  'bet.below_minimum': 'error_bet_below_minimum',
  'bet.side_locked': 'error_bet_side_locked',
  'bet.too_many_bets': 'error_bet_too_many_bets',
  'bet.pool_full': 'error_bet_pool_full',
  'bet.fight_cap_exceeded': 'error_bet_fight_cap_exceeded',
  'bet.insufficient_balance': 'error_bet_insufficient_balance',
  'bet.unavailable': 'error_bet_unavailable',
  'bonus.already_claimed': 'error_bonus_already_claimed',
  'bailout.grant_available': 'error_bailout_grant_available',
  'bailout.not_eligible': 'error_bailout_not_eligible',
  'bailout.already_claimed': 'error_bailout_already_claimed',
  'wallet.unavailable': 'error_wallet_unavailable',
  'season.not_found': 'error_season_not_found',
  'season.unavailable': 'error_season_unavailable',
  'botarga.not_created': 'error_botarga_not_created',
  'botarga.invalid_request': 'error_botarga_invalid_request',
  'botarga.not_unlocked': 'error_botarga_not_unlocked',
  'botarga.style_unknown': 'error_botarga_style_unknown',
  'botarga.budget_exceeded': 'error_botarga_budget_exceeded',
  'botarga.stat_out_of_range': 'error_botarga_stat_out_of_range',
  'botarga.slots_locked': 'error_botarga_slots_locked',
  'botarga.ability_count': 'error_botarga_ability_count',
  'botarga.ability_kind_count': 'error_botarga_ability_kind_count',
  'botarga.ability_duplicate': 'error_botarga_ability_duplicate',
  'botarga.weakness_count': 'error_botarga_weakness_count',
  'botarga.accessory_slot_duplicate': 'error_botarga_accessory_slot_duplicate',
  'botarga.accessory_stat_total': 'error_botarga_accessory_stat_total',
  'botarga.build_invalid': 'error_botarga_build_invalid',
  'botarga.unavailable': 'error_botarga_unavailable',
  'pvp.invalid_request': 'error_pvp_invalid_request',
  'pvp.not_found': 'error_pvp_not_found',
  'pvp.build_invalid': 'error_pvp_build_invalid',
  'pvp.no_rival': 'error_pvp_no_rival',
  'pvp.daily_limit': 'error_pvp_daily_limit',
  'pvp.busy': 'error_pvp_busy',
  'pvp.unavailable': 'error_pvp_unavailable',
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
  'auth.sessions_not_closed': 'error_auth_sessions_not_closed',
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
