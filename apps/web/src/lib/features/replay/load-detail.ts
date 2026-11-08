import type { FightDetail } from '@torneo/protocol';

// What the browser reads of a fight (011 RF13, RF18, RF24). `parseFightDetail` only checks the shape:
// it never looks at whether it knows a `state`, a `kind` or a `reason`, so what is new reaches
// `verifyFight` and gets its own verdict.

const LOAD_TIMEOUT_MS = 10_000;

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);
const isText = (value: unknown): value is string => typeof value === 'string';
const isTextOrNull = (value: unknown): boolean => value === null || isText(value);
const isNumber = (value: unknown): boolean => typeof value === 'number' && Number.isFinite(value);

function isFighter(value: unknown): boolean {
  return isRecord(value) && isText(value.slug) && isNumber(value.max_hp);
}

function isFinal(value: unknown): boolean {
  return value === null || (isRecord(value) && isText(value.kind));
}

function isRuleset(value: unknown): boolean {
  return Number.isInteger(value) && (value as number) >= 0 && (value as number) <= 65_535;
}

function isSummary(value: unknown): boolean {
  if (!isRecord(value)) return false;
  return (
    Number.isInteger(value.id) &&
    isText(value.state) &&
    isFighter(value.a) &&
    isFighter(value.b) &&
    isRuleset(value.ruleset_version) &&
    isText(value.snapshot_sha256) &&
    isTextOrNull(value.seed_commit) &&
    isText(value.opens_at) &&
    isText(value.closes_at) &&
    isText(value.live_at) &&
    isTextOrNull(value.live_started_at) &&
    isNumber(value.time_limit_ms) &&
    isNumber(value.max_duration_ms) &&
    isNumber(value.playback_delay_ms) &&
    isFinal(value.final)
  );
}

/** The value as a `FightDetail` if it has its shape; otherwise null. */
export function parseFightDetail(value: unknown): FightDetail | null {
  if (!isRecord(value) || !isSummary(value.fight)) return null;
  const shaped =
    isTextOrNull(value.spec_a) &&
    isTextOrNull(value.spec_b) &&
    isTextOrNull(value.seed) &&
    isTextOrNull(value.log);
  return shaped ? (value as FightDetail) : null;
}

/**
 * Whether anything of this response can be shown (011 RF24, security review of the amendment): it
 * has the shape, it is the fight of the URL and it says it is over. Otherwise only the `mismatch`
 * shows. A fight that has not settled never gets here: the SSR gives its own view, without a read.
 */
export function isTrustworthy(value: unknown, expectedId: number): value is FightDetail {
  const detail = parseFightDetail(value);
  return (
    detail !== null &&
    detail.fight.id === expectedId &&
    detail.fight.state === 'settled' &&
    detail.fight.final !== null
  );
}

export type LoadedDetail =
  /** JSON, whatever it says: `verifyFight` and `isTrustworthy` decide. */
  | { readonly kind: 'loaded'; readonly body: unknown }
  /** The network, a limit, an error or a 200 that is not JSON: a retry, and no verdict at all. */
  | { readonly kind: 'unavailable' };

/** The one read of a page: the player, the result and the verdict all come from this response. */
export async function loadDetail(id: number, fetcher: typeof fetch): Promise<LoadedDetail> {
  try {
    const response = await fetcher(`/api/fights/${id}`, {
      headers: { accept: 'application/json' },
      cache: 'no-store',
      signal: AbortSignal.timeout(LOAD_TIMEOUT_MS),
    });
    if (response.status !== 200) return { kind: 'unavailable' };
    return { kind: 'loaded', body: (await response.json()) as unknown };
  } catch {
    return { kind: 'unavailable' };
  }
}
