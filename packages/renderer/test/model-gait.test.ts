import { describe, expect, it } from 'vitest';
import {
  arrivingTime,
  leavingTime,
  rootMotion,
  stepDistance,
  strideOf,
  timeAtDistance,
  type RootMotion,
} from '../src/model/gait.ts';

/** A step like the botarga's: 1 s, the root eases 0.2 m forward and settles at the end. */
const STEP = rootMotion([0, 0.25, 0.5, 0.75, 1], [0.05, 0.1, 0.2, 0.25, 0.25]) as RootMotion;

describe('rootMotion', () => {
  it('measures the advance from the first keyframe, whichever way the clip walks', () => {
    expect(STEP.distance).toEqual([0, 0.05, 0.15, 0.2, 0.2].map((d) => expect.closeTo(d)));
    const back = rootMotion([0, 0.5, 1], [-0.05, -0.15, -0.25]) as RootMotion;
    expect(back.distance).toEqual([0, 0.1, 0.2].map((d) => expect.closeTo(d)));
    expect(strideOf(back)).toBeCloseTo(0.2);
  });

  it('never goes back when the root wavers', () => {
    const motion = rootMotion([0, 1, 2, 3], [0, 0.1, 0.08, 0.2]) as RootMotion;
    expect(motion.distance).toEqual([0, 0.1, 0.1, 0.2].map((d) => expect.closeTo(d)));
  });

  it('is null for a clip in place or unusable data', () => {
    expect(rootMotion([0, 1], [0.3, 0.301])).toBeNull();
    expect(rootMotion([0], [0])).toBeNull();
    expect(rootMotion([0, 1], [0])).toBeNull();
    expect(rootMotion([0, 0], [0, 1])).toBeNull();
    expect(rootMotion([0, 1], [0, Number.NaN])).toBeNull();
  });
});

describe('timeAtDistance', () => {
  it('is the inverse of the root motion', () => {
    expect(timeAtDistance(STEP, 0)).toBe(0);
    expect(timeAtDistance(STEP, 0.05)).toBeCloseTo(0.25);
    expect(timeAtDistance(STEP, 0.1)).toBeCloseTo(0.375);
    expect(timeAtDistance(STEP, 0.175)).toBeCloseTo(0.625);
    // The first frame that reaches the stride: the settled tail adds no ground.
    expect(timeAtDistance(STEP, 0.2)).toBeCloseTo(0.75);
  });

  it('clamps to the clip', () => {
    expect(timeAtDistance(STEP, -1)).toBe(0);
    expect(timeAtDistance(STEP, 5)).toBeCloseTo(0.75);
  });

  it('grows with the distance, so a planted foot never slides back', () => {
    let last = -1;
    for (let d = 0; d <= 0.2; d += 0.002) {
      const time = timeAtDistance(STEP, d);
      expect(time).toBeGreaterThanOrEqual(last);
      last = time;
    }
  });
});

describe('steps counted back from the arrival', () => {
  it('starts a trip of whole strides at the start of a step', () => {
    expect(stepDistance(0.2, 0.2)).toBeCloseTo(0);
    expect(stepDistance(0.4, 0.2)).toBeCloseTo(0);
    expect(stepDistance(0.6000000001, 0.2)).toBeCloseTo(0);
  });

  it('ends every step at a multiple of the stride from the destination', () => {
    expect(stepDistance(0.15, 0.2)).toBeCloseTo(0.05);
    expect(stepDistance(0.001, 0.2)).toBeCloseTo(0.199);
    expect(stepDistance(0.25, 0.2)).toBeCloseTo(0.15);
    expect(stepDistance(0.21, 0.2)).toBeCloseTo(0.19);
  });

  it('starts a shorter trip partway into its only step', () => {
    expect(stepDistance(0.19, 0.2)).toBeCloseTo(0.01);
    expect(stepDistance(5, 0)).toBe(0);
  });

  it('plays one clip per stride of ground on the way in and on the way out', () => {
    // Arriving: 0.2 m left is the first frame, almost there is the last moving frame.
    expect(arrivingTime(STEP, 0.2)).toBeCloseTo(0);
    expect(arrivingTime(STEP, 0.1)).toBeCloseTo(0.375);
    expect(arrivingTime(STEP, 0.0001)).toBeCloseTo(0.75, 2);
    // Leaving: the same frames by the ground covered, looping every stride.
    expect(leavingTime(STEP, 0.1)).toBeCloseTo(0.375);
    expect(leavingTime(STEP, 0.3)).toBeCloseTo(0.375);
  });
});
