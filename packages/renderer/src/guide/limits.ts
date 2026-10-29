// The hand-picked ranges of the sprite guide (007 RF-A2, spec "Guía: números"), approved by
// game-designer. The only place in code where they live. Derived bounds (the pivot, the intro
// cap, the reaction window, the knockdown and ko caps, the reach and the windup cap) are not
// fields: `table.ts` computes them from their sources.

const animations = Object.freeze({
  idle: Object.freeze({
    framesMin: 4,
    framesMax: 12,
    distinctMin: 3,
    frameMaxMs: 250,
    cycleMinMs: 600,
    cycleMaxMs: 2400,
  }),
  walk: Object.freeze({ framesMin: 4, framesMax: 12, cycleMinMs: 300, cycleMaxMs: 1200 }),
  intro: Object.freeze({ framesMin: 3, framesMax: 24 }),
  attack_light: Object.freeze({
    framesMin: 3,
    framesMax: 8,
    windupMinMs: 100,
    windupMaxMs: 250,
    impactFrameMin: 1,
    impactHoldMinMs: 50,
  }),
  attack_heavy: Object.freeze({
    framesMin: 4,
    framesMax: 10,
    windupMinMs: 250,
    impactFrameMin: 2,
    impactHoldMinMs: 66,
  }),
  special: Object.freeze({
    framesMin: 5,
    framesMax: 14,
    windupMinMs: 300,
    impactFrameMin: 3,
    impactHoldMinMs: 66,
  }),
  hit: Object.freeze({ framesMin: 2, framesMax: 6, totalMaxMs: 1000 }),
  block: Object.freeze({ framesMin: 2, framesMax: 6, totalMaxMs: 1000 }),
  dodge: Object.freeze({ framesMin: 2, framesMax: 6, totalMaxMs: 1000 }),
  knockdown: Object.freeze({ framesMin: 3, framesMax: 12 }),
  ko: Object.freeze({ framesMin: 3, framesMax: 12 }),
  win: Object.freeze({ framesMin: 2, framesMax: 12 }),
});

export const GUIDE_LIMITS = Object.freeze({
  /** Side of the square cell every frame measures. */
  cellPx: 192,
  /** Rows below the feet, for a shadow or dust. */
  groundMarginPx: 8,
  /** Opaque height of `idle` step 0. */
  heightMinPx: 64,
  heightMaxPx: 176,
  /** The lowest opaque row of each `idle` frame is in [pivotY − 1 − this, pivotY − 1]. */
  feetTolerancePx: 2,
  frameMsMin: 33,
  frameMsMax: 1000,
  /** Shortest time a key pose has to be seen. */
  keyPoseMinMs: 66,
  /** Minimum windup difference between `attack_light` and `attack_heavy` in one set. */
  windupStepMs: 100,
  sheetFileMaxBytes: 4 * 1024 * 1024,
  exportFileMaxBytes: 1024 * 1024,
  animations,
});
