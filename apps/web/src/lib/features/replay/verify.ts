import type { FightDetail, FightFinal } from '@torneo/protocol';
import { parseFightDetail } from './load-detail.js';

// What the browser proves of a settled fight (011 RF11 to RF13): the same steps, in the same order,
// as apps/server/tests/support/public_verifier.rs. Pure: no DOM, no Svelte and no glue, everything
// by `deps`. It compares what the engine gives and never simulates by itself. What depends on the
// published data is a `mismatch`; what depends on the browser, a `cannot_check` (RF12).

export type VerifyCheck =
  'data' | 'state' | 'commit' | 'snapshot' | 'log' | 'final' | 'summary' | 'bounds';
export type CannotCheckReason =
  | 'ruleset_unknown'
  | 'engine_unavailable'
  | 'engine_failed'
  | 'crypto_unavailable'
  | 'unknown_final';

export type Verdict =
  | { readonly status: 'match'; readonly scope: 'fight' | 'commit' | 'snapshot' }
  | { readonly status: 'mismatch'; readonly check: VerifyCheck }
  | { readonly status: 'cannot_check'; readonly reason: CannotCheckReason }
  | { readonly status: 'not_applicable' };

export interface VerifyEngine {
  simulate(seedHex: string, a: string, b: string, rulesetVersion: number): string;
  rulesetBounds(rulesetVersion: number): string;
}

export interface VerifyDeps {
  /** Throws `CryptoUnavailableError` outside a secure context: there is no fallback (RF13). */
  sha256(bytes: Uint8Array<ArrayBuffer>): Promise<ArrayBuffer | Uint8Array>;
  /** Rejects if it cannot be loaded in time (RF5). Only called on a `decided` past RF2. */
  engine(): Promise<VerifyEngine>;
}

export class CryptoUnavailableError extends Error {}

const SEED = /^[0-9a-f]{64}$/;
/** The codes that the published data causes; `ruleset.unknown` and the rest are not. */
const DATA_CODE = /^(?:seed\.|spec\.|input\.too_large$)/;

const mismatch = (check: VerifyCheck): Verdict => ({ status: 'mismatch', check });
const cannotCheck = (reason: CannotCheckReason): Verdict => ({ status: 'cannot_check', reason });
const matched = (scope: 'fight' | 'commit' | 'snapshot'): Verdict => ({ status: 'match', scope });
const UNKNOWN_FINAL = cannotCheck('unknown_final');

function hexOf(bytes: ArrayBuffer | Uint8Array): string {
  return Array.from(new Uint8Array(bytes), (byte) => byte.toString(16).padStart(2, '0')).join('');
}

function bytesOfHex(hex: string): Uint8Array<ArrayBuffer> {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i++) bytes[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return bytes;
}

/** Whether the published seed is the one of the published commit. */
async function commitHolds(detail: FightDetail, deps: VerifyDeps): Promise<boolean> {
  const { seed } = detail;
  const commit = detail.fight.seed_commit;
  if (seed === null || commit === null || !SEED.test(seed)) return false;
  return hexOf(await deps.sha256(bytesOfHex(seed))) === commit;
}

/** RF2, by concatenation, as any language would. */
async function snapshotHolds(detail: FightDetail, deps: VerifyDeps): Promise<boolean> {
  const { spec_a: a, spec_b: b } = detail;
  if (a === null || b === null) return false;
  const text = `{"ruleset_version":${detail.fight.ruleset_version},"a":${a},"b":${b}}`;
  const hash = await deps.sha256(new TextEncoder().encode(text));
  return hexOf(hash) === detail.fight.snapshot_sha256;
}

/** A call into the engine: what it returned, or the verdict its error means. */
function callEngine(call: () => string): { text: string } | { verdict: Verdict } {
  try {
    return { text: call() };
  } catch (error) {
    const code = error instanceof Error ? error.message : '';
    if (code === 'ruleset.unknown') return { verdict: cannotCheck('ruleset_unknown') };
    if (DATA_CODE.test(code)) return { verdict: mismatch('log') };
    // A trap of the `.wasm`, or `log.encode_failed`.
    return { verdict: cannotCheck('engine_failed') };
  }
}

interface LogHeader {
  readonly ruleset_version: number;
  readonly a: { readonly slug: string; readonly max_hp: number };
  readonly b: { readonly slug: string; readonly max_hp: number };
  readonly events: readonly {
    readonly type: string;
    readonly winner?: unknown;
    readonly reason?: unknown;
  }[];
}

type Decided = Extract<FightFinal, { kind: 'decided' }>;

async function verifyDecided(
  detail: FightDetail,
  final: Decided,
  deps: VerifyDeps,
): Promise<Verdict> {
  const { fight, seed, spec_a: specA, spec_b: specB, log: logText } = detail;
  if (!(await commitHolds(detail, deps)) || seed === null) return mismatch('commit');
  if (!(await snapshotHolds(detail, deps)) || specA === null || specB === null)
    return mismatch('snapshot');
  if (logText === null) return mismatch('log');

  let engine: VerifyEngine;
  try {
    engine = await deps.engine();
  } catch {
    return cannotCheck('engine_unavailable');
  }

  const simulated = callEngine(() => engine.simulate(seed, specA, specB, fight.ruleset_version));
  if ('verdict' in simulated) return simulated.verdict;
  if (simulated.text !== logText) return mismatch('log');

  // The text is what the engine just wrote, and the specs are what it just accepted.
  const log = JSON.parse(logText) as LogHeader;
  const end = log.events.findLast((event) => event.type === 'end');
  if (end?.winner !== final.winner || end?.reason !== final.reason) return mismatch('final');

  const a = JSON.parse(specA) as { readonly slug: string };
  const b = JSON.parse(specB) as { readonly slug: string };
  if (
    fight.a.slug !== a.slug ||
    fight.b.slug !== b.slug ||
    fight.a.slug !== log.a.slug ||
    fight.b.slug !== log.b.slug ||
    fight.a.max_hp !== log.a.max_hp ||
    fight.b.max_hp !== log.b.max_hp ||
    fight.ruleset_version !== log.ruleset_version
  ) {
    return mismatch('summary');
  }

  const bounds = callEngine(() => engine.rulesetBounds(fight.ruleset_version));
  if ('verdict' in bounds) {
    // Nothing of the data reaches this call but the version: only an unknown one is a code here.
    return bounds.verdict.status === 'mismatch' ? cannotCheck('engine_failed') : bounds.verdict;
  }
  const limits = JSON.parse(bounds.text) as {
    readonly time_limit_ms: number;
    readonly max_duration_ms: number;
  };
  if (
    fight.time_limit_ms !== limits.time_limit_ms ||
    fight.max_duration_ms !== limits.max_duration_ms
  ) {
    return mismatch('bounds');
  }
  return matched('fight');
}

async function verifyVoided(detail: FightDetail, checkSnapshot: boolean, deps: VerifyDeps) {
  if (!(await commitHolds(detail, deps))) return mismatch('commit');
  if (detail.log !== null) return mismatch('log');
  if (checkSnapshot && !(await snapshotHolds(detail, deps))) return mismatch('snapshot');
  return matched('commit');
}

async function verifyCancelled(detail: FightDetail, checkSnapshot: boolean, deps: VerifyDeps) {
  if (detail.seed !== null || detail.fight.seed_commit !== null) return mismatch('commit');
  if (detail.log !== null) return mismatch('log');
  // A corrupt snapshot may be why it could not run, and it never had a seed.
  if (!checkSnapshot) return { status: 'not_applicable' } as const;
  if (!(await snapshotHolds(detail, deps))) return mismatch('snapshot');
  return matched('snapshot');
}

async function verifyDetail(detail: FightDetail, deps: VerifyDeps): Promise<Verdict> {
  const final = detail.fight.final as FightFinal;
  // Exhaustive by `never`, so a new variant does not compile; at run time, an old build's verdict.
  switch (final.kind) {
    case 'decided': {
      const { reason } = final;
      switch (reason) {
        case 'ko':
        case 'decision':
        case 'draw':
          return verifyDecided(detail, final, deps);
        default:
          return unknownFinal(reason);
      }
    }
    case 'voided': {
      const { reason } = final;
      switch (reason) {
        case 'snapshot_invalid':
          return verifyVoided(detail, false, deps);
        case 'engine_error':
          return verifyVoided(detail, true, deps);
        default:
          return unknownFinal(reason);
      }
    }
    case 'cancelled': {
      const { reason } = final;
      switch (reason) {
        case 'fight_unrunnable':
          return verifyCancelled(detail, false, deps);
        case 'missed_window':
          return verifyCancelled(detail, true, deps);
        default:
          return unknownFinal(reason);
      }
    }
    default:
      return unknownFinal(final);
  }
}

function unknownFinal(_value: never): Verdict {
  return UNKNOWN_FINAL;
}

/** Never rejects: any unexpected error is a `cannot_check`, so the view never stays in "checking". */
export async function verifyFight(
  body: unknown,
  expectedId: number,
  deps: VerifyDeps,
): Promise<Verdict> {
  try {
    const detail = parseFightDetail(body);
    if (detail === null || detail.fight.id !== expectedId) return mismatch('data');
    if (detail.fight.state !== 'settled' || detail.fight.final === null) return mismatch('state');
    return await verifyDetail(detail, deps);
  } catch (error) {
    return cannotCheck(
      error instanceof CryptoUnavailableError ? 'crypto_unavailable' : 'engine_failed',
    );
  }
}

/** What the view offers with a `cannot_check` (RF15). A reload is what helps after a trap. */
export type VerdictAction = 'retry' | 'reload' | 'none';

export function verdictAction(verdict: Verdict): VerdictAction {
  if (verdict.status !== 'cannot_check') return 'none';
  switch (verdict.reason) {
    case 'engine_unavailable':
      return 'retry';
    case 'ruleset_unknown':
    case 'unknown_final':
    case 'engine_failed':
      return 'reload';
    case 'crypto_unavailable':
      return 'none';
  }
}
