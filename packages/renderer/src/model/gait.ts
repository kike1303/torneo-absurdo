// The step of a walk clip tied to the ground it covers (TA-74). The botarga's walks are guard
// steps with their own root motion: each foot moves once and the hips advance one stride per clip.
// Played against the clock while the scene moved the fighter several times faster, the feet slid.
// Here the clip time is the inverse of that root motion: the clip shows the frame in which its
// hips have advanced exactly what the fighter has travelled, so a planted foot stays planted.
// Pure: plain numbers in, plain numbers out.

/** How far the root of a clip has advanced along its travel at each keyframe. */
export interface RootMotion {
  /** Keyframe times in seconds, increasing. */
  readonly times: readonly number[];
  /** Meters advanced at each keyframe: starts at 0 and never decreases. */
  readonly distance: readonly number[];
}

export type WalkClip = 'walk_forward' | 'walk_back';
/** The root motion of each walk clip that has one, read from the GLB by the Three layer. */
export type Gaits = Readonly<Partial<Record<WalkClip, RootMotion>>>;

/** Below this stride (meters) a clip walks in place: there is no root motion to follow. */
export const MIN_STRIDE_M = 0.01;
/** Keeps a trip of exactly N strides at N steps, whatever the rounding of its length. */
const WHOLE_STEP_EPSILON = 1e-6;

/**
 * The root motion of a clip from the signed advance of its root along the facing (`advance`, one
 * value per keyframe; a walk back advances negatively). A root that wavers never goes back: the
 * distance is the furthest it has been. Null when the clip walks in place or the data is unusable.
 */
export function rootMotion(
  times: readonly number[],
  advance: readonly number[],
): RootMotion | null {
  const count = times.length;
  if (count < 2 || advance.length !== count) return null;
  const start = advance[0] as number;
  const direction = Math.sign((advance[count - 1] as number) - start);
  const distance: number[] = [];
  let furthest = 0;
  for (let i = 0; i < count; i++) {
    const d = ((advance[i] as number) - start) * direction;
    if (!Number.isFinite(d) || !Number.isFinite(times[i])) return null;
    if (i > 0 && (times[i] as number) <= (times[i - 1] as number)) return null;
    furthest = Math.max(furthest, d);
    distance.push(furthest);
  }
  return furthest < MIN_STRIDE_M ? null : { times: [...times], distance };
}

/** Meters one play of the clip covers. */
export function strideOf(motion: RootMotion): number {
  return motion.distance[motion.distance.length - 1] ?? 0;
}

/** Seconds into the clip when its root has first advanced `meters` (clamped to the stride). */
export function timeAtDistance(motion: RootMotion, meters: number): number {
  const { times, distance } = motion;
  const d = Math.min(strideOf(motion), Math.max(0, meters));
  let i = 0;
  while (i < distance.length - 1 && (distance[i] as number) < d) i++;
  if (i === 0) return times[0] ?? 0;
  const d0 = distance[i - 1] as number;
  const d1 = distance[i] as number;
  const t0 = times[i - 1] as number;
  const t1 = times[i] as number;
  return d1 > d0 ? t0 + ((d - d0) / (d1 - d0)) * (t1 - t0) : t1;
}

/**
 * Meters into the current step with `remaining` meters left to walk: the steps are counted back
 * from the arrival, so the last one always ends there on the clip's last frame (the guard the
 * idle starts from). A trip that is not a whole number of strides starts partway into a step.
 */
export function stepDistance(remaining: number, stride: number): number {
  if (stride <= 0) return 0;
  const left = Math.max(0, remaining);
  const steps = Math.ceil(left / stride - WHOLE_STEP_EPSILON);
  return Math.min(stride, Math.max(0, steps * stride - left));
}

/** Seconds into a walk clip with `remaining` meters left to its destination. */
export function arrivingTime(motion: RootMotion, remaining: number): number {
  return timeAtDistance(motion, stepDistance(remaining, strideOf(motion)));
}

/** Seconds into a walk clip `travelled` meters after leaving: the clip loops every stride. */
export function leavingTime(motion: RootMotion, travelled: number): number {
  const stride = strideOf(motion);
  return timeAtDistance(motion, stride > 0 ? Math.max(0, travelled) % stride : 0);
}
