// Animation timing and loops (002 RF-B5, plan-b decision 9).
import { LIMITS, LOOPING_ANIMATIONS, type AnimationCode } from '@torneo/renderer/manifest';
import type { Note } from './errors.ts';
import type { AirAction, AirFrame } from './air.ts';

/** `ticks = -1` is held: the last frame of a non-looping animation, with a nominal duration. */
export const HOLD_MS = 100;
const TICK_MS = 1000 / 60;

export interface TimedFrame {
  readonly frame: AirFrame;
  readonly ms: number;
}

export function frameMs(ticks: number, notes: Note[], animation: AnimationCode): number {
  if (ticks === -1) return HOLD_MS;
  if (ticks === 0) {
    notes.push({ code: 'convert.zero_ticks', data: { animation } });
    return Math.round(TICK_MS);
  }
  const ms = Math.round(ticks * TICK_MS);
  const clamped = Math.min(LIMITS.frameMsMax, Math.max(LIMITS.frameMsMin, ms));
  if (clamped !== ms) notes.push({ code: 'convert.frame_ms_clamped', data: { animation, ms } });
  return clamped;
}

export function isLooping(code: AnimationCode): boolean {
  return LOOPING_ANIMATIONS.includes(code);
}

/**
 * The frames of one of our animations from its MUGEN actions, played one after the other, with
 * every adjustment noted. A truncated chain keeps its last frame: it is the pose the renderer
 * holds (the fighter lying down, in a knockdown).
 */
export function timedFrames(
  code: AnimationCode,
  actions: readonly AirAction[],
  notes: Note[],
): TimedFrame[] {
  let timed = actions.flatMap((action) => actionFrames(code, action, notes));
  const last = actions.length > 1 ? timed.pop() : undefined;
  const room = last === undefined ? 0 : 1;
  if (timed.length + room > LIMITS.animationFramesMax) {
    notes.push({
      code: 'convert.animation_truncated',
      data: { animation: code, frames: timed.length + room },
    });
    timed = timed.slice(0, LIMITS.animationFramesMax - room);
  }
  if (!isLooping(code)) {
    const reserved = last?.ms ?? 0;
    let total = timed.reduce((sum, t) => sum + t.ms, 0) + reserved;
    if (total > LIMITS.nonLoopingTotalMaxMs) {
      notes.push({ code: 'convert.animation_truncated', data: { animation: code, ms: total } });
      while (timed.length > 1 - room && total > LIMITS.nonLoopingTotalMaxMs) {
        total -= timed.pop()?.ms ?? 0;
      }
    }
  }
  if (last !== undefined) timed.push(last);
  return timed;
}

function actionFrames(code: AnimationCode, action: AirAction, notes: Note[]): TimedFrame[] {
  let frames = [...action.frames];
  const hold = frames.findIndex((f) => f.ticks === -1);
  if (hold !== -1 && hold < frames.length - 1) {
    notes.push({
      code: 'convert.unreachable_frames',
      data: { animation: code, frames: frames.length - hold - 1 },
    });
    frames = frames.slice(0, hold + 1);
  }
  const loop = isLooping(code);
  if (
    loop &&
    action.loopStart !== null &&
    action.loopStart > 0 &&
    action.loopStart < frames.length
  ) {
    frames = frames.slice(action.loopStart);
  }
  return frames.map((frame) => ({ frame, ms: frameMs(frame.ticks, notes, code) }));
}
