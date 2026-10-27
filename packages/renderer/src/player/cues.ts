// Cue dispatch without hidden state (plan-a2 decision 3): a cue fires once when the playback
// passes its `at_ms`, in order. `seek` moves the cursor without firing what it skips.
import { lowerBound, upperBound } from '../scene/prepare.ts';

export interface DueCues {
  /** First index to fire (inclusive). */
  readonly from: number;
  /** Last index to fire (exclusive); also the next cursor. */
  readonly to: number;
}

/**
 * Cues due at `t`, given the cursor `next` (the first cue not fired yet). A big jump of the
 * clock, like coming back to a hidden tab, returns every cue of the interval.
 */
export function dueCues(cueTimes: readonly number[], next: number, t: number): DueCues {
  const to = Math.max(next, upperBound(cueTimes, t));
  return { from: next, to };
}

/** Cursor after a seek to `t`: cues at exactly `t` still fire on the next playing tick. */
export function cursorAt(cueTimes: readonly number[], t: number): number {
  return lowerBound(cueTimes, t);
}

/** Index of the last cue with `at_ms ≤ t`, or −1. */
export function lastCueAt(cueTimes: readonly number[], t: number): number {
  return upperBound(cueTimes, t) - 1;
}
