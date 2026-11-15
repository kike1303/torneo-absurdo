import type { FightEvent, FightLog, PvpFightDetail } from '@torneo/protocol';
import { roundClockMs } from '../fight/live/round-clock.js';
import { timelineFromFightLog, type AdaptedTimeline } from '../fight/timeline-from-fight-log.js';
import type { ReplayLoad, ReplaySnapshot } from '../replay/replay-store.svelte.js';
import type { Verdict, VerifyDeps } from '../replay/verify.js';
import { parsePvpDetail, verifyPvpFight } from './verify.js';

// The replay of a player fight (021, with the rules of 011 RF18 to RF24): one read in the browser,
// and the player, the result and the verdict all speak of that response. Starts paused at t = 0;
// the result waits for the end of the fight or the button.

const ASSETS = { a: { asset: 'dummy_a' }, b: { asset: 'dummy_b' } } as const;
const LOAD_TIMEOUT_MS = 10_000;

export type PvpLoaded =
  { readonly kind: 'loaded'; readonly body: unknown } | { readonly kind: 'unavailable' };

/** Where a replay reads its fight: a public player fight (021), or a private one between friends
 * (023), whose answer wraps the same detail in `{ fight }`. */
export type PvpReplayKind = 'pvp' | 'friends';

/** The detail inside a body of `kind`, as the server sent it (still unchecked). */
export function detailBody(kind: PvpReplayKind, body: unknown): unknown {
  if (kind === 'pvp') return body;
  return typeof body === 'object' && body !== null && 'fight' in body
    ? (body as { fight: unknown }).fight
    : null;
}

/** The one read of a page. */
export async function loadPvpDetail(
  id: number,
  fetcher: typeof fetch,
  kind: PvpReplayKind = 'pvp',
): Promise<PvpLoaded> {
  try {
    const path = kind === 'pvp' ? `/api/pvp/fights/${id}` : `/api/friends/fights/${id}`;
    const response = await fetcher(path, {
      headers: { accept: 'application/json' },
      cache: 'no-store',
      signal: AbortSignal.timeout(LOAD_TIMEOUT_MS),
    });
    if (response.status !== 200) return { kind: 'unavailable' };
    return { kind: 'loaded', body: detailBody(kind, (await response.json()) as unknown) };
  } catch {
    return { kind: 'unavailable' };
  }
}

export class PvpReplayStore {
  load = $state<ReplayLoad>('loading');
  detail = $state.raw<PvpFightDetail | null>(null);
  verdict = $state.raw<Verdict | null>(null);
  timeline = $state.raw<AdaptedTimeline | null>(null);
  playbackError = $state(false);
  noAnimation = $state(false);
  resultSeen = $state(false);

  /** The time limit of the fight's ruleset, from the engine; the clock waits for it. */
  private timeLimitMs = $state<number | null>(null);
  private snapshot = $state.raw<ReplaySnapshot | null>(null);
  private events = $state.raw<readonly FightEvent[]>([]);
  private body: unknown = null;

  constructor(
    private readonly id: number,
    private readonly fetcher: typeof fetch,
    private readonly deps: VerifyDeps,
    private readonly kind: PvpReplayKind = 'pvp',
  ) {}

  readonly stageVisible = $derived(
    this.timeline !== null && !this.playbackError && !this.noAnimation,
  );

  readonly roundClock = $derived.by((): number | null => {
    if (this.detail === null || !this.stageVisible || this.timeLimitMs === null) return null;
    return roundClockMs(this.timeLimitMs, this.events, this.snapshot?.t ?? 0);
  });

  source = (): Promise<AdaptedTimeline> => {
    const timeline = this.timeline;
    return timeline === null
      ? Promise.reject(new Error('timeline_invalid'))
      : Promise.resolve(timeline);
  };

  onSnapshot = (snapshot: ReplaySnapshot | null): void => {
    this.snapshot = snapshot;
    if (snapshot?.ending != null || snapshot?.phase === 'finished') this.resultSeen = true;
  };

  onStageFailure = (code: string): void => {
    if (code === 'webgl_unavailable') this.noAnimation = true;
    else this.playbackError = true;
  };

  showResult = (): void => {
    this.resultSeen = true;
  };

  retryVerification = (): void => {
    void this.verify();
  };

  start = async (): Promise<void> => {
    this.load = 'loading';
    const loaded = await loadPvpDetail(this.id, this.fetcher, this.kind);
    if (loaded.kind === 'unavailable') {
      this.load = 'unavailable';
      return;
    }
    this.body = loaded.body;
    const detail = parsePvpDetail(loaded.body);
    if (detail !== null && detail.id === this.id) {
      this.detail = detail;
      this.prepare(detail);
      this.load = 'ready';
      void this.bounds(detail.ruleset_version);
    } else {
      this.load = 'untrusted';
    }
    await this.verify();
  };

  /** The round clock of a player fight is the one of its ruleset, as the engine bounds it. */
  private async bounds(version: number): Promise<void> {
    try {
      const engine = await this.deps.engine();
      const limits = JSON.parse(engine.rulesetBounds(version)) as { time_limit_ms?: unknown };
      if (typeof limits.time_limit_ms === 'number') this.timeLimitMs = limits.time_limit_ms;
    } catch {
      // Without the engine there is no clock; the replay plays all the same.
    }
  }

  private prepare(detail: PvpFightDetail): void {
    try {
      const log = JSON.parse(detail.log) as FightLog;
      const adapted = timelineFromFightLog(log, ASSETS);
      if (!adapted.ok) throw new Error('refused');
      this.events = log.events;
      this.timeline = adapted.timeline;
    } catch {
      this.playbackError = true;
    }
  }

  private async verify(): Promise<void> {
    this.verdict = null;
    this.verdict = await verifyPvpFight(this.body, this.id, this.deps);
  }
}
