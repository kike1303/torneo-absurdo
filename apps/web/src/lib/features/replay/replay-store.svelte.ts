import type { FightDetail, FightEvent, FightLog } from '@torneo/protocol';
import { resultOfFinal, type FightResult } from '../fight/live/view-state.js';
import { roundClockMs } from '../fight/live/round-clock.js';
import { timelineFromFightLog, type AdaptedTimeline } from '../fight/timeline-from-fight-log.js';
import { isTrustworthy, loadDetail } from './load-detail.js';
import { verifyFight, type Verdict, type VerifyDeps } from './verify.js';

// The page of a fight (011 RF15, RF18 to RF24). One read in the browser: the player, the result and
// the verdict all speak of that same response. The decisions are here, and the components only draw.

/** The placeholders, one per side (ADR-0004). */
const ASSETS = { a: { asset: 'dummy_a' }, b: { asset: 'dummy_b' } } as const;

export type ReplayLoad =
  | 'loading'
  /** The API or the network: a retry, and no verdict at all. */
  | 'unavailable'
  /** Nothing of the response can be shown: no player, no result, no fields; only its `mismatch`. */
  | 'untrusted'
  | 'ready';

/** What the store reads of the renderer, by structure, without knowing it (fight-boundary.test.ts). */
export interface ReplaySnapshot {
  readonly t: number;
  readonly phase: string;
  readonly ending: string | null;
}

/** A final this build can name; at run time a `kind` or a `reason` may be new (RF22). */
function resultOf(detail: FightDetail): FightResult | null {
  const { fight } = detail;
  const final = fight.final;
  if (final === null) return null;
  if (final.kind === 'decided') {
    const known: readonly string[] = ['ko', 'decision', 'draw'];
    return known.includes(final.reason) ? resultOfFinal(fight, final) : null;
  }
  if (final.kind === 'cancelled' || final.kind === 'voided') return resultOfFinal(fight, final);
  return null;
}

export class ReplayStore {
  load = $state<ReplayLoad>('loading');
  detail = $state.raw<FightDetail | null>(null);
  /** Null while it is checking. */
  verdict = $state.raw<Verdict | null>(null);
  timeline = $state.raw<AdaptedTimeline | null>(null);
  /** The log does not parse, is missing or was refused; or the stage failed. */
  playbackError = $state(false);
  /** No WebGL, or the context lost too often. */
  noAnimation = $state(false);
  /** Seen or asked for: it stays when the fight restarts (RF20). */
  resultSeen = $state(false);

  private snapshot = $state.raw<ReplaySnapshot | null>(null);
  private events = $state.raw<readonly FightEvent[]>([]);
  private body: unknown = null;

  constructor(
    private readonly id: number,
    private readonly fetcher: typeof fetch,
    private readonly deps: VerifyDeps,
  ) {}

  /** The final the fight has, from the response of the player. */
  readonly result = $derived(this.detail === null ? null : resultOf(this.detail));

  /** A `decided` fight is the one with something to play. */
  readonly decided = $derived(this.detail?.fight.final?.kind === 'decided');

  readonly stageVisible = $derived(
    this.timeline !== null && !this.playbackError && !this.noAnimation,
  );

  readonly roundClock = $derived.by((): number | null => {
    const detail = this.detail;
    if (detail === null || !this.stageVisible) return null;
    return roundClockMs(detail.fight.time_limit_ms, this.events, this.snapshot?.t ?? 0);
  });

  /** Where the timeline of the stage comes from: it is already adapted. */
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

  /** Only `engine_unavailable` offers it: after a trap the glue keeps its instance (RF15). */
  retryVerification = (): void => {
    void this.verify();
  };

  /** The one read, and a new one only on a retry after `unavailable`. */
  start = async (): Promise<void> => {
    this.load = 'loading';
    const loaded = await loadDetail(this.id, this.fetcher);
    if (loaded.kind === 'unavailable') {
      this.load = 'unavailable';
      return;
    }
    this.body = loaded.body;
    if (isTrustworthy(loaded.body, this.id)) {
      this.detail = loaded.body;
      this.prepare(loaded.body);
      this.load = 'ready';
    } else {
      this.load = 'untrusted';
    }
    await this.verify();
  };

  private prepare(detail: FightDetail): void {
    if (detail.fight.final?.kind !== 'decided') return;
    try {
      if (detail.log === null) throw new Error('no log');
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
    this.verdict = await verifyFight(this.body, this.id, this.deps);
  }
}
