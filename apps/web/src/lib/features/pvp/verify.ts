import type { EndReason, PvpFightDetail, PvpFighter, Side } from '@torneo/protocol';
import type { Verdict, VerifyCheck, VerifyDeps, VerifyEngine } from '../replay/verify.js';
import { IDLE_CLIPS, VICTORY_CLIPS } from '../botarga/catalog.js';

// What the browser proves of a player fight (021 "Lectura", 011): its log is what the engine gives
// with its own published seed and specs. There is no commit nor snapshot to check: a player fight
// has no betting window, so its seed is public from the moment it exists. Pure, as `verifyFight`:
// everything that is not data comes by `deps`.

const SEED = /^[0-9a-f]{64}$/;
const DATA_CODE = /^(?:seed\.|spec\.|input\.too_large$)/;
const SIDES: readonly string[] = ['a', 'b'];
const REASONS: readonly string[] = ['ko', 'decision', 'draw'];

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);
const isText = (value: unknown): value is string => typeof value === 'string';

function isFighter(value: unknown): value is PvpFighter {
  if (!isRecord(value)) return false;
  if (value.kind === 'player') return isText(value.display_name);
  if (value.kind === 'tournament') return isText(value.slug);
  return false;
}

function isLook(value: unknown): boolean {
  if (value === null) return true;
  if (!isRecord(value)) return false;
  const clip = (v: unknown, codes: readonly string[]) =>
    v === null || (isText(v) && codes.includes(v));
  return (
    clip(value.idle, IDLE_CLIPS) &&
    clip(value.victory, VICTORY_CLIPS) &&
    Array.isArray(value.accessories) &&
    value.accessories.every(isText)
  );
}

/** The value as a `PvpFightDetail` if it has its shape; otherwise null. */
export function parsePvpDetail(value: unknown): PvpFightDetail | null {
  if (!isRecord(value)) return null;
  const shaped =
    Number.isInteger(value.id) &&
    isFighter(value.a) &&
    isFighter(value.b) &&
    isText(value.spec_a) &&
    isText(value.spec_b) &&
    isText(value.seed) &&
    isText(value.log) &&
    Number.isInteger(value.ruleset_version) &&
    (value.winner === null || (isText(value.winner) && SIDES.includes(value.winner))) &&
    isText(value.end_reason) &&
    REASONS.includes(value.end_reason) &&
    Number.isInteger(value.rating_delta) &&
    isLook(value.look_a) &&
    isLook(value.look_b) &&
    isText(value.created_at);
  return shaped ? (value as unknown as PvpFightDetail) : null;
}

const mismatch = (check: VerifyCheck): Verdict => ({ status: 'mismatch', check });

function callEngine(call: () => string): { text: string } | { verdict: Verdict } {
  try {
    return { text: call() };
  } catch (error) {
    const code = error instanceof Error ? error.message : '';
    if (code === 'ruleset.unknown') {
      return { verdict: { status: 'cannot_check', reason: 'ruleset_unknown' } };
    }
    if (DATA_CODE.test(code)) return { verdict: mismatch('log') };
    return { verdict: { status: 'cannot_check', reason: 'engine_failed' } };
  }
}

interface LogHeader {
  readonly ruleset_version: number;
  readonly a: { readonly slug: string };
  readonly b: { readonly slug: string };
  readonly events: readonly {
    readonly type: string;
    readonly winner?: unknown;
    readonly reason?: unknown;
  }[];
}

async function verifyDetail(detail: PvpFightDetail, deps: VerifyDeps): Promise<Verdict> {
  const { seed, spec_a: specA, spec_b: specB, log: logText } = detail;
  if (!SEED.test(seed)) return mismatch('commit');
  let engine: VerifyEngine;
  try {
    engine = await deps.engine();
  } catch {
    return { status: 'cannot_check', reason: 'engine_unavailable' };
  }
  const simulated = callEngine(() => engine.simulate(seed, specA, specB, detail.ruleset_version));
  if ('verdict' in simulated) return simulated.verdict;
  if (simulated.text !== logText) return mismatch('log');

  // The text is what the engine just wrote, and the specs are what it just accepted.
  const log = JSON.parse(logText) as LogHeader;
  const end = log.events.findLast((event) => event.type === 'end');
  const winner: Side | null = detail.winner;
  const reason: EndReason = detail.end_reason;
  if ((end?.winner ?? null) !== winner || end?.reason !== reason) return mismatch('final');
  const a = JSON.parse(specA) as { readonly slug: string };
  const b = JSON.parse(specB) as { readonly slug: string };
  if (
    a.slug !== log.a.slug ||
    b.slug !== log.b.slug ||
    detail.ruleset_version !== log.ruleset_version ||
    (detail.b.kind === 'tournament' && detail.b.slug !== b.slug)
  ) {
    return mismatch('summary');
  }
  return { status: 'match', scope: 'fight' };
}

/** Never rejects: any unexpected error is a `cannot_check`, so the view never stays "checking". */
export async function verifyPvpFight(
  body: unknown,
  expectedId: number,
  deps: VerifyDeps,
): Promise<Verdict> {
  try {
    const detail = parsePvpDetail(body);
    if (detail === null || detail.id !== expectedId) return mismatch('data');
    return await verifyDetail(detail, deps);
  } catch {
    return { status: 'cannot_check', reason: 'engine_failed' };
  }
}
