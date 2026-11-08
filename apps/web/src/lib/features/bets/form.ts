import type { BetRequest, ErrorCode, Side } from '@torneo/protocol';
import { isErrorCode } from '#lib/features/auth/codes.js';

// 012 RF44: what the bet form posts. Only shape: the server checks the window, the minimum, the caps
// and the balance, and its answer is the only one the panel shows.

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const DIGITS = /^[0-9]{1,15}$/;

export const isSide = (value: unknown): value is Side => value === 'a' || value === 'b';

/** A whole number of coins as typed, or `null`. */
export function parseAmount(value: unknown): number | null {
  if (typeof value !== 'string') return null;
  const text = value.trim();
  if (!DIGITS.test(text)) return null;
  const amount = Number(text);
  return Number.isSafeInteger(amount) ? amount : null;
}

/** The bet of a posted form, or `null` if its shape is wrong. */
export function parseBetForm(form: FormData): BetRequest | null {
  const fight = form.get('fight_id');
  const side = form.get('side');
  const key = form.get('client_key');
  const amount = parseAmount(form.get('amount'));
  if (typeof fight !== 'string' || !DIGITS.test(fight)) return null;
  const fightId = Number(fight);
  if (!Number.isSafeInteger(fightId) || fightId > 2_147_483_647) return null;
  if (!isSide(side) || typeof key !== 'string' || !UUID.test(key) || amount === null) return null;
  return { fight_id: fightId, side, amount, client_key: key };
}

/** What the `?/bet` action keeps on a refusal (RF42): the code, and what was typed. */
export interface BetFormFailure {
  readonly code: ErrorCode | null;
  readonly amount: string;
  readonly side: Side | null;
  readonly fight: number | null;
}

/** The refusal of `?/bet` in the `form` of the page, or `null` for any other result. */
export function betFailureOf(form: unknown): BetFormFailure | null {
  if (typeof form !== 'object' || form === null || !('amount' in form)) return null;
  const { code, amount, side, fight } = form as Record<string, unknown>;
  return {
    code: isErrorCode(code) ? code : null,
    amount: typeof amount === 'string' ? amount : '',
    side: isSide(side) ? side : null,
    fight: typeof fight === 'number' && Number.isSafeInteger(fight) ? fight : null,
  };
}
