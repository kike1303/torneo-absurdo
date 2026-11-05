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

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

const isDurations = (v: unknown): v is number[] =>
  Array.isArray(v) && v.length > 0 && v.every((n) => typeof n === 'number' && n > 0);

/** `frames` scaled to sum `target`, in whole ms inside the manifest limits; the last one absorbs
 * the rounding so the sum is exact. */
function stretch(frames: readonly number[], target: number): number[] {
  const sum = frames.reduce((a, b) => a + b, 0);
  const clamp = (n: number) => Math.min(LIMITS.frameMsMax, Math.max(LIMITS.frameMsMin, n));
  const out = frames.map((ms) => clamp(Math.round((ms * target) / sum)));
  const rest = target - out.slice(0, -1).reduce((a, b) => a + b, 0);
  out[out.length - 1] = clamp(rest);
  return out;
}

function paceAnimation(animation: unknown, pace: ActionPace): unknown {
  if (!isRecord(animation) || !isDurations(animation.frame_ms)) return animation;
  const frames = animation.frame_ms;
  const impact = animation.impact_frame;
  if (
    pace.windup !== undefined &&
    typeof impact === 'number' &&
    Number.isInteger(impact) &&
    impact > 0 &&
    impact < frames.length
  ) {
    return {
      ...animation,
      frame_ms: [
        ...stretch(frames.slice(0, impact), pace.windup),
        ...stretch(frames.slice(impact), pace.total - pace.windup),
      ],
    };
  }
  return { ...animation, frame_ms: stretch(frames, pace.total) };
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
