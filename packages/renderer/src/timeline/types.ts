// Renderer timeline v1 (spec "Diseño → Timeline v1", RF-A7). Codes only, no text (ADR-0007).

export const TIMELINE_VERSION = 1;

export const SIDES = ['a', 'b'] as const;
export type Side = (typeof SIDES)[number];

export const MOVES = ['attack_light', 'attack_heavy', 'special'] as const;
export type Move = (typeof MOVES)[number];

export const STRIKE_RESULTS = ['hit', 'crit', 'block', 'dodge'] as const;
export type StrikeResult = (typeof STRIKE_RESULTS)[number];

export const CUE_TYPES = ['intro', 'strike', 'knockdown', 'ko', 'win'] as const;
export type CueType = (typeof CUE_TYPES)[number];

export const TIMELINE_LIMITS = Object.freeze({
  maxCues: 2000,
  maxDurationMs: 3_600_000,
  maxHp: 1_000_000,
  introGapMs: 2000,
  strikeGapMs: 600,
  knockdownLagMs: 100,
  knockdownMinMs: 800,
  knockdownMaxMs: 3000,
  winGapMs: 1500,
  winPoseMs: 2000,
  maxErrors: 100,
});

export interface IntroCue {
  readonly at_ms: number;
  readonly type: 'intro';
}

export interface StrikeCue {
  /** The instant of the impact. */
  readonly at_ms: number;
  readonly type: 'strike';
  readonly actor: Side;
  readonly move: Move;
  readonly result: StrikeResult;
  readonly damage: number;
  /** HP of the target (the other side) after the strike. */
  readonly target_hp: number;
}

export interface KnockdownCue {
  readonly at_ms: number;
  readonly type: 'knockdown';
  readonly fighter: Side;
  readonly duration_ms: number;
}

export interface KoCue {
  readonly at_ms: number;
  readonly type: 'ko';
  readonly fighter: Side;
}

export interface WinCue {
  readonly at_ms: number;
  readonly type: 'win';
  readonly fighter: Side;
}

export type Cue = IntroCue | StrikeCue | KnockdownCue | KoCue | WinCue;

export interface TimelineFighter {
  readonly asset: string;
  readonly max_hp: number;
}

export interface Timeline {
  readonly timeline_version: typeof TIMELINE_VERSION;
  readonly duration_ms: number;
  readonly fighters: Readonly<Record<Side, TimelineFighter>>;
  readonly cues: readonly Cue[];
}

export type TimelineErrorCode = `timeline.${string}`;

export interface TimelineError {
  readonly code: TimelineErrorCode;
  readonly path: string;
  readonly data?: Readonly<Record<string, string | number>>;
}

export function otherSide(side: Side): Side {
  return side === 'a' ? 'b' : 'a';
}
