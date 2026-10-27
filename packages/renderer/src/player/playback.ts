// Playback clock over the host clock (plan-a2 decision 2). Pure: `play`, `pause` and `seek`
// only move the anchor, and `t` is always read from the clock, never counted in frames.

export interface Playback {
  readonly playing: boolean;
  readonly anchorClock: number;
  readonly anchorT: number;
}

/** The identity anchor: with it, `t = clock()` (live playback never pauses nor seeks). */
export const INITIAL_PLAYBACK: Playback = Object.freeze({
  playing: false,
  anchorClock: 0,
  anchorT: 0,
});

function clampT(t: number, durationMs: number): number {
  if (Number.isNaN(t)) return 0;
  return Math.min(durationMs, Math.max(0, t));
}

export function timeAt(playback: Playback, now: number, durationMs: number): number {
  const t = playback.playing ? playback.anchorT + (now - playback.anchorClock) : playback.anchorT;
  return clampT(t, durationMs);
}

/**
 * Starts from where the playback stands. From the initial state the anchor stays the identity,
 * so a host whose clock is already the timeline's time base (live) gets `t = clock()`.
 */
export function play(playback: Playback, now: number, durationMs: number): Playback {
  if (playback.playing) return playback;
  if (playback === INITIAL_PLAYBACK) return { ...INITIAL_PLAYBACK, playing: true };
  return { playing: true, anchorClock: now, anchorT: timeAt(playback, now, durationMs) };
}

export function pause(playback: Playback, now: number, durationMs: number): Playback {
  if (!playback.playing) return playback;
  return { playing: false, anchorClock: now, anchorT: timeAt(playback, now, durationMs) };
}

export function seek(playback: Playback, now: number, ms: number, durationMs: number): Playback {
  return { playing: playback.playing, anchorClock: now, anchorT: clampT(ms, durationMs) };
}
