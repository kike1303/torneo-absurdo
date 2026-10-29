// Codes of the sprite guide (007 RF-A3, spec "Guía: reglas validadas") and of the ruleset golden
// parser (RF-A5). Codes, never text (ADR-0007).
export const GUIDE_CODES = Object.freeze({
  noRulesets: 'guide.no_rulesets',
  scaleForbidden: 'guide.scale_forbidden',
  cellSize: 'guide.cell_size',
  pivotPosition: 'guide.pivot_position',
  frameMsRange: 'guide.frame_ms_range',
  framesCount: 'guide.frames_count',
  idleDistinct: 'guide.idle_distinct',
  idleFrameLong: 'guide.idle_frame_long',
  cycleRange: 'guide.cycle_range',
  durationRange: 'guide.duration_range',
  windupShort: 'guide.windup_short',
  windupLong: 'guide.windup_long',
  windupOrder: 'guide.windup_order',
  anticipation: 'guide.anticipation',
  impactHold: 'guide.impact_hold',
  recoveryShort: 'guide.recovery_short',
  reactionSlow: 'guide.reaction_slow',
  frameEmpty: 'guide.frame_empty',
  frameClipped: 'guide.frame_clipped',
  feetOffPivot: 'guide.feet_off_pivot',
  heightRange: 'guide.height_range',
  impactShort: 'guide.impact_short',
  /** The cut of `ErrorSink` at `LIMITS.maxErrors`; not a rule of the guide. */
  tooManyErrors: 'guide.too_many_errors',
} as const);

export const RULESET_CODES = Object.freeze({ invalid: 'ruleset.invalid' } as const);
