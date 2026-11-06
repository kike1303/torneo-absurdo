// Renderer timeline v1 (002 spec "Diseño → Timeline v1", RF-A7), v2 (005 spec, RF1) and v3 (015):
// each is a strict superset of the one before. v2 adds the endings by time (`time_up`, then `win`
// or `draw`); v3 adds the optional `ability` of a strike. Codes only, no text (ADR-0007).

export const TIMELINE_VERSIONS = [1, 2, 3] as const;
export type TimelineVersion = (typeof TIMELINE_VERSIONS)[number];
/** The latest version, the one producers emit. */
export const TIMELINE_VERSION = 3;

/** An ability id of the engine (v3): a code, never text. */
export const ABILITY_PATTERN = /^[a-z0-9_]{1,64}$/;

export const SIDES = ['a', 'b'] as const;
export type Side = (typeof SIDES)[number];

export const MOVES = ['attack_light', 'attack_heavy', 'special'] as const;
export type Move = (typeof MOVES)[number];

export const STRIKE_RESULTS = ['hit', 'crit', 'block', 'dodge'] as const;
export type StrikeResult = (typeof STRIKE_RESULTS)[number];

export const CUE_TYPES_V1 = ['intro', 'strike', 'knockdown', 'ko', 'win'] as const;
export const CUE_TYPES = [...CUE_TYPES_V1, 'time_up', 'draw'] as const;
export type CueType = (typeof CUE_TYPES)[number];

/** How a fight ended: knockout, decision after the time limit, or draw. */
export const ENDINGS = ['ko', 'decision', 'draw'] as const;
export type Ending = (typeof ENDINGS)[number];

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
  /** The ability behind the strike (v3, optional): the 3D arena gives each its own clip (015). */
  readonly ability?: string;
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

/** The time limit is reached (v2). The closing (`win` or `draw`) follows. */
export interface TimeUpCue {
  readonly at_ms: number;
  readonly type: 'time_up';
}

/** A draw after `time_up` (v2). */
export interface DrawCue {
  readonly at_ms: number;
  readonly type: 'draw';
}

export type Cue = IntroCue | StrikeCue | KnockdownCue | KoCue | WinCue | TimeUpCue | DrawCue;

export interface TimelineFighter {
  readonly asset: string;
  readonly max_hp: number;
}

export interface Timeline {
  readonly timeline_version: TimelineVersion;
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
