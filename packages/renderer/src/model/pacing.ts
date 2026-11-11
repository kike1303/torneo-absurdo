// The botarga's own pace (017). The sprite manifests are short, made for a few drawn frames: fit to
// them, a Mixamo clip plays 4 to 6 times its natural speed. The 3D layer stretches each action
// towards its clip, with the windups inside `LIMITS.windupMaxMs` so the choreography guarantee of
// 002 holds (the reaction to a strike is always seen). Pure: it only rewrites `frame_ms`.
import { LIMITS } from '../manifest/types.ts';

export interface ActionPace {
  /** ms from the start to the impact (attacks only). */
  readonly windup?: number;
  readonly total: number;
}

export const BOTARGA_PACE: Readonly<Record<string, ActionPace>> = Object.freeze({
  attack_light: { windup: 260, total: 800 },
  attack_heavy: { windup: 360, total: 1100 },
  special: { windup: LIMITS.windupMaxMs, total: 1300 },
  hit: { total: 750 },
  block: { total: 650 },
  dodge: { total: 750 },
});

/**
 * How long a botarga takes back to its mark (TA-74): three guard steps (`STEP_M`) at a pace that
 * reads as steps. The 2D return (300 ms) crossed them at ~2 m/s, feet flickering. The fight does
 * not slow down: the cues stay where they are and the next strike cuts a return short.
 */
export const BOTARGA_RETURN_MS = 900;

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

const isDurations = (v: unknown): v is number[] =>
  Array.isArray(v) && v.length > 0 && v.every((n) => typeof n === 'number' && n > 0);

const sum = (ms: readonly number[]) => ms.reduce((a, b) => a + b, 0);

/**
 * `frames` scaled to sum exactly `target`, every frame inside the manifest limits; null when no
 * such split exists (too many frames for the target). After rounding, the difference goes 1 ms at
 * a time to the longest frames that can still take it (review of #99: clamping the last frame to
 * the minimum used to overshoot the target).
 */
export function stretch(frames: readonly number[], target: number): number[] | null {
  const { frameMsMin: min, frameMsMax: max } = LIMITS;
  if (target < frames.length * min || target > frames.length * max) return null;
  const total = sum(frames);
  const out = frames.map((ms) => Math.min(max, Math.max(min, Math.round((ms * target) / total))));
  let diff = target - sum(out);
  while (diff !== 0) {
    const step = Math.sign(diff);
    let pick = -1;
    for (let i = 0; i < out.length; i++) {
      const room = step > 0 ? (out[i] as number) < max : (out[i] as number) > min;
      if (room && (pick === -1 || (out[i] as number) > (out[pick] as number))) pick = i;
    }
    if (pick === -1) return null;
    out[pick] = (out[pick] as number) + step;
    diff -= step;
  }
  return out;
}

function paceAnimation(animation: unknown, pace: ActionPace): unknown {
  if (!isRecord(animation) || !isDurations(animation.frame_ms)) return animation;
  const frames = animation.frame_ms;
  const impact = animation.impact_frame;
  let paced: number[] | null;
  if (
    pace.windup !== undefined &&
    typeof impact === 'number' &&
    Number.isInteger(impact) &&
    impact > 0 &&
    impact < frames.length
  ) {
    const windup = stretch(frames.slice(0, impact), pace.windup);
    const rest = stretch(frames.slice(impact), pace.total - pace.windup);
    paced = windup === null || rest === null ? null : [...windup, ...rest];
    // The choreography guarantee (002) caps every windup: a manifest it would break stays as is.
    if (paced !== null && sum(paced.slice(0, impact)) > LIMITS.windupMaxMs) paced = null;
  } else {
    paced = stretch(frames, pace.total);
  }
  return paced === null ? animation : { ...animation, frame_ms: paced };
}

/**
 * The manifest with the botarga's pace. Anything it does not recognize goes through unchanged, so
 * `validateAssets` still reports it.
 */
export function paceManifest(manifest: unknown): unknown {
  if (!isRecord(manifest) || !isRecord(manifest.animations)) return manifest;
  const animations: Record<string, unknown> = { ...manifest.animations };
  for (const [code, pace] of Object.entries(BOTARGA_PACE)) {
    if (code in animations) animations[code] = paceAnimation(animations[code], pace);
  }
  return { ...manifest, animations };
}

export function paceAssets(assets: ReadonlyMap<string, unknown>): ReadonlyMap<string, unknown> {
  return new Map([...assets].map(([id, manifest]) => [id, paceManifest(manifest)]));
}
