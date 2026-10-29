import { describe, expect, it } from 'vitest';
import { GUIDE_LIMITS } from '../src/guide/index.ts';
import {
  LIMITS,
  OPTIONAL_ANIMATIONS,
  REACTION_WINDOW_MS,
  REQUIRED_ANIMATIONS,
} from '../src/manifest/index.ts';
import { CONTACT_PX } from '../src/scene/index.ts';
import { TIMELINE_LIMITS } from '../src/timeline/index.ts';

// 007 RF-A2: GUIDE_LIMITS holds the approved values and nothing else.
const A = GUIDE_LIMITS.animations;

describe('GUIDE_LIMITS', () => {
  it('is deeply frozen', () => {
    expect(Object.isFrozen(GUIDE_LIMITS)).toBe(true);
    expect(Object.isFrozen(A)).toBe(true);
    for (const row of Object.values(A)) expect(Object.isFrozen(row)).toBe(true);
  });

  it('has exactly the keys of the plan', () => {
    expect(Object.keys(GUIDE_LIMITS).sort()).toEqual(
      [
        'cellPx',
        'groundMarginPx',
        'heightMinPx',
        'heightMaxPx',
        'feetTolerancePx',
        'frameMsMin',
        'frameMsMax',
        'keyPoseMinMs',
        'windupStepMs',
        'sheetFileMaxBytes',
        'exportFileMaxBytes',
        'animations',
      ].sort(),
    );
    expect(Object.keys(A).sort()).toEqual([...REQUIRED_ANIMATIONS, ...OPTIONAL_ANIMATIONS].sort());
  });

  it('holds the approved values', () => {
    expect(GUIDE_LIMITS).toEqual({
      cellPx: 192,
      groundMarginPx: 8,
      heightMinPx: 64,
      heightMaxPx: 176,
      feetTolerancePx: 2,
      frameMsMin: 33,
      frameMsMax: 1000,
      keyPoseMinMs: 66,
      windupStepMs: 100,
      sheetFileMaxBytes: 4 * 1024 * 1024,
      exportFileMaxBytes: 1024 * 1024,
      animations: {
        idle: {
          framesMin: 4,
          framesMax: 12,
          distinctMin: 3,
          frameMaxMs: 250,
          cycleMinMs: 600,
          cycleMaxMs: 2400,
        },
        walk: { framesMin: 4, framesMax: 12, cycleMinMs: 300, cycleMaxMs: 1200 },
        intro: { framesMin: 3, framesMax: 24 },
        attack_light: {
          framesMin: 3,
          framesMax: 8,
          windupMinMs: 100,
          windupMaxMs: 250,
          impactFrameMin: 1,
          impactHoldMinMs: 50,
        },
        attack_heavy: {
          framesMin: 4,
          framesMax: 10,
          windupMinMs: 250,
          impactFrameMin: 2,
          impactHoldMinMs: 66,
        },
        special: {
          framesMin: 5,
          framesMax: 14,
          windupMinMs: 300,
          impactFrameMin: 3,
          impactHoldMinMs: 66,
        },
        hit: { framesMin: 2, framesMax: 6, totalMaxMs: 1000 },
        block: { framesMin: 2, framesMax: 6, totalMaxMs: 1000 },
        dodge: { framesMin: 2, framesMax: 6, totalMaxMs: 1000 },
        knockdown: { framesMin: 3, framesMax: 12 },
        ko: { framesMin: 3, framesMax: 12 },
        win: { framesMin: 2, framesMax: 12 },
      },
    });
  });

  it('is internally coherent', () => {
    for (const row of Object.values(A)) expect(row.framesMin).toBeLessThanOrEqual(row.framesMax);
    const G = GUIDE_LIMITS;
    expect(G.heightMaxPx).toBeLessThanOrEqual(G.cellPx - G.groundMarginPx - 8);
    expect(G.heightMinPx).toBeLessThanOrEqual(G.heightMaxPx);
    expect(G.cellPx / 2).toBeGreaterThan(CONTACT_PX);
    // The windup order can always be met under the structural cap.
    expect(A.attack_light.windupMaxMs + G.windupStepMs).toBeLessThanOrEqual(LIMITS.windupMaxMs);
    expect(G.keyPoseMinMs).toBeLessThan(REACTION_WINDOW_MS);
    expect(G.frameMsMin).toBeGreaterThanOrEqual(33);
    expect(A.idle.frameMaxMs * A.idle.framesMax).toBeGreaterThanOrEqual(A.idle.cycleMinMs);
  });

  it('gives the derived values the spec quotes', () => {
    const pivot = [GUIDE_LIMITS.cellPx / 2, GUIDE_LIMITS.cellPx - GUIDE_LIMITS.groundMarginPx];
    expect(pivot).toEqual([96, 184]);
    expect(TIMELINE_LIMITS.introGapMs - LIMITS.windupMaxMs).toBe(1600);
  });
});
