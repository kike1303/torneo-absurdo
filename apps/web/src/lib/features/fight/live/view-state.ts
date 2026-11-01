import type { FightFinal, FightSummary, FightsCurrent, Side } from '@torneo/protocol';
import type { EndOutcome } from './text-scene.js';

// One function decides what the home page shows (010 RF54, plan-d decision 4). Pure: everything
// comes in the arguments, so every row has its test.

/** The pose stays this long after `end.t`. */
export const POSE_AFTER_END_MS = 10_000;

export type ViewKind =
  'error' | 'halted' | 'loading' | 'starting' | 'live' | 'about_to_start' | 'next' | 'result';

/** The fight with an open session. */
export interface Watched {
  readonly summary: FightSummary;
  /** Playback time; negative before the fight is due on screen. Null without `live_started_at`. */
  readonly t: number | null;
  /** `end` already received, whatever `t` says. */
  readonly end: (EndOutcome & { readonly t: number }) | null;
  /** The closing already on screen, from `snapshot().ending` or by `t`. */
  readonly ending: EndOutcome | null;
  /** First frame painted, or text mode active. */
  readonly painted: boolean;
  /** Without canvas: no WebGL, the context lost twice, a rejection or a `fail`. */
  readonly textMode: boolean;
  /** When it got up to date, `end.t` had already gone by: no Pixi at all. */
  readonly lateNoPixi: boolean;
  /** A received event is still ahead of `t`. */
  readonly pending: boolean;
}

export interface ViewInput {
  readonly board: FightsCurrent | null;
  readonly watched: Watched | null;
  readonly serverNow: number;
  readonly degraded: boolean;
  readonly ssrFailed: boolean;
}

export type ResultKind = 'ko' | 'decision' | 'draw' | 'cancelled' | 'voided';

export interface FightResult {
  readonly fight: FightSummary;
  readonly kind: ResultKind;
  readonly winner: Side | null;
}

export interface View {
  readonly kind: ViewKind;
  /** The fight of `live`. */
  readonly live: FightSummary | null;
  /** The fight of the countdown. */
  readonly upcoming: FightSummary | null;
  /** Its target, in server time. */
  readonly target: number | null;
  /** The previous fight, or the one just seen, once its result can show. */
  readonly result: FightResult | null;
  /** `live` with the label "live" on: off as soon as there is an `ending`. */
  readonly liveBadge: boolean;
  /** `none`: not mounted; `hidden`: mounted and loading, with its size; `visible`. */
  readonly stage: 'none' | 'hidden' | 'visible';
  readonly showRoundClock: boolean;
}

const targetOf = (fight: FightSummary): number =>
  Number(fight.live_started_at ?? fight.live_at) + fight.playback_delay_ms;

/** Whether the `final` of a fight can show (RF54, E4-6). */
export function finalVisible(fight: FightSummary, watched: Watched | null): boolean {
  const final = fight.final;
  if (final === null) return false;
  if (watched === null || watched.summary.id !== fight.id) return true;
  if (final.kind !== 'decided') return true;
  return watched.t !== null && watched.t >= fight.max_duration_ms;
}

export function watchedFinished(watched: Watched): boolean {
  const { summary, end, t } = watched;
  if (end !== null && t !== null && t >= end.t + POSE_AFTER_END_MS) return true;
  if (watched.lateNoPixi) return true;
  const final = summary.final;
  if (final === null || !finalVisible(summary, watched)) return false;
  return end === null || final.kind !== 'decided';
}

function resultOfFinal(fight: FightSummary, final: FightFinal): FightResult {
  if (final.kind === 'cancelled') return { fight, kind: 'cancelled', winner: null };
  if (final.kind === 'voided') return { fight, kind: 'voided', winner: null };
  return { fight, kind: final.reason, winner: final.winner };
}

function resultOf(input: ViewInput): FightResult | null {
  const { watched } = input;
  const last = input.board?.last ?? null;
  if (watched !== null && (last === null || last.id <= watched.summary.id)) {
    const fight = watched.summary;
    if (watched.end !== null && watched.t !== null && watched.t >= watched.end.t) {
      return { fight, kind: watched.end.reason, winner: watched.end.winner };
    }
    if (watched.ending !== null) {
      return { fight, kind: watched.ending.reason, winner: watched.ending.winner };
    }
    if (fight.final !== null && finalVisible(fight, watched)) {
      return resultOfFinal(fight, fight.final);
    }
    if (last === null || last.id === fight.id) return null;
  }
  if (last === null || last.final === null || !finalVisible(last, watched)) return null;
  return resultOfFinal(last, last.final);
}

export function viewState(input: ViewInput): View {
  const { board, watched, serverNow, degraded } = input;
  const finished = watched !== null && watchedFinished(watched);
  const result = resultOf(input);
  const current = board?.current ?? null;
  const next = board?.next ?? null;
  const last = board?.last ?? null;
  const nothing = current === null && next === null && last === null;
  const halted = board?.halted ?? null;

  const inLive = (fight: FightSummary): boolean =>
    fight.live_started_at !== null &&
    serverNow >= Number(fight.live_started_at) + fight.playback_delay_ms;
  const upcoming = current !== null && !inLive(current) ? current : next;
  // Mounted while it has something to show, and never for a fight already seen.
  const mounted = watched !== null && !finished && !watched.textMode;
  const base: View = {
    kind: 'starting',
    live: null,
    upcoming: null,
    target: null,
    result,
    liveBadge: false,
    stage: mounted ? 'hidden' : 'none',
    showRoundClock: false,
  };

  if (input.ssrFailed && board === null) return { ...base, kind: 'error' };

  const onScreen =
    watched !== null && !finished && watched.t !== null && watched.t >= 0 && watched.painted;

  if (halted === true) {
    // The canvas stays only while there is something left to play.
    const playing = onScreen && watched.pending;
    return { ...base, kind: 'halted', stage: mounted && playing ? 'visible' : 'none' };
  }

  if (nothing && watched === null) {
    return { ...base, kind: halted === null ? 'loading' : 'starting' };
  }

  if (onScreen) {
    return {
      ...base,
      kind: 'live',
      // `last` stays only until the next fight is live (RF54).
      result: result?.fight.id === watched.summary.id ? result : null,
      live: watched.summary,
      liveBadge: watched.ending === null,
      stage: mounted ? 'visible' : 'none',
      showRoundClock: !degraded,
    };
  }

  const watchedIsCurrent = watched !== null && current?.id === watched.summary.id;
  if (current !== null && current.state === 'live' && inLive(current)) {
    // Degraded, no event will come: it must not wait for a frame that needs them.
    if (!watchedIsCurrent || (degraded && !finished)) {
      return {
        ...base,
        kind: 'live',
        result: null,
        live: current,
        liveBadge: true,
        showRoundClock: !degraded,
      };
    }
    if (!finished) {
      return { ...base, kind: 'about_to_start', upcoming: current, target: targetOf(current) };
    }
  }

  if (upcoming !== null) {
    const target = targetOf(upcoming);
    const kind = serverNow >= target ? 'about_to_start' : 'next';
    return { ...base, kind, upcoming, target };
  }

  if (result !== null || last !== null || finished) return { ...base, kind: 'result' };
  return base;
}
