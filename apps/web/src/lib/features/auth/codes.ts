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
  'auth.sessions_not_closed': true,
  'profile.display_name_invalid': true,
  'profile.display_name_reserved': true,
  'profile.display_name_taken': true,
  'fight.not_found': true,
  'fight.unavailable': true,
  'bet.invalid_request': true,
  'bet.key_reused': true,
  'bet.window_closed': true,
  'bet.below_minimum': true,
  'bet.side_locked': true,
  'bet.too_many_bets': true,
  'bet.pool_full': true,
  'bet.fight_cap_exceeded': true,
  'bet.insufficient_balance': true,
  'bet.unavailable': true,
  'bonus.already_claimed': true,
  'bailout.grant_available': true,
  'bailout.not_eligible': true,
  'bailout.already_claimed': true,
  'wallet.unavailable': true,
  'season.not_found': true,
  'season.unavailable': true,
  'botarga.not_created': true,
  'botarga.invalid_request': true,
  'botarga.not_unlocked': true,
  'botarga.style_unknown': true,
  'botarga.budget_exceeded': true,
  'botarga.stat_out_of_range': true,
  'botarga.slots_locked': true,
  'botarga.ability_count': true,
  'botarga.ability_kind_count': true,
  'botarga.ability_duplicate': true,
  'botarga.weakness_count': true,
  'botarga.accessory_slot_duplicate': true,
  'botarga.accessory_stat_total': true,
  'botarga.build_invalid': true,
  'botarga.unavailable': true,
  'botarga.draw_not_found': true,
  'botarga.draw_already_chosen': true,
  'botarga.draw_option_invalid': true,
  'pvp.invalid_request': true,
  'pvp.not_found': true,
  'pvp.build_invalid': true,
  'pvp.no_rival': true,
  'pvp.daily_limit': true,
  'pvp.busy': true,
  'pvp.unavailable': true,
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
  'auth.sessions_not_closed': true,
};

/** Only own keys: `__proto__`, `constructor` or `toString` are never codes. */
export function isErrorCode(value: unknown): value is ErrorCode {
  return typeof value === 'string' && Object.hasOwn(ERROR_CODES, value);
}

export function isCallbackErrorCode(value: unknown): value is CallbackErrorCode {
  return typeof value === 'string' && Object.hasOwn(CALLBACK_ERROR_CODES, value);
}
