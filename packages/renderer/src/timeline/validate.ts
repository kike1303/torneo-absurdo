// validateTimeline (RF-A7, plan-a1 decision 14). Pure; the input is untrusted.
import {
  ErrorSink,
  StopValidation,
  isInt,
  isPlainObject,
  own,
  pointer,
} from '../manifest/input.ts';
import { LIMITS as MANIFEST_LIMITS, type SpriteManifest } from '../manifest/types.ts';
import { TIMELINE_CODES as C } from './codes.ts';
import {
  CUE_TYPES,
  MOVES,
  SIDES,
  STRIKE_RESULTS,
  TIMELINE_LIMITS as L,
  TIMELINE_VERSION,
  otherSide,
  type Cue,
  type CueType,
  type Side,
  type StrikeCue,
  type Timeline,
  type TimelineError,
  type TimelineErrorCode,
  type TimelineFighter,
} from './types.ts';

export type TimelineResult =
  | { readonly ok: true; readonly timeline: Timeline }
  | { readonly ok: false; readonly errors: readonly TimelineError[] };

/** Manifests by asset id (a Map, so no id can collide with a prototype key). */
export type ManifestsById = ReadonlyMap<string, SpriteManifest>;

type Sink = ErrorSink<TimelineErrorCode>;

const ROOT_FIELDS = ['timeline_version', 'duration_ms', 'fighters', 'cues'];
const FIGHTER_FIELDS = ['asset', 'max_hp'];
const CUE_FIELDS: Readonly<Record<CueType, readonly string[]>> = {
  intro: ['at_ms', 'type'],
  strike: ['at_ms', 'type', 'actor', 'move', 'result', 'damage', 'target_hp'],
  knockdown: ['at_ms', 'type', 'fighter', 'duration_ms'],
  ko: ['at_ms', 'type', 'fighter'],
  win: ['at_ms', 'type', 'fighter'],
};

function oneOf<T extends string>(list: readonly T[], value: unknown): value is T {
  return typeof value === 'string' && (list as readonly string[]).includes(value);
}

function unknownFields(
  sink: Sink,
  obj: Record<string, unknown>,
  allowed: readonly string[],
  ...path: (string | number)[]
): void {
  for (const key of Object.keys(obj)) {
    if (!allowed.includes(key)) sink.add(C.unknownField, pointer(...path, key));
  }
}

function checkFighters(
  sink: Sink,
  value: unknown,
  manifests: ManifestsById | undefined,
): Record<Side, TimelineFighter> | undefined {
  if (value === undefined) {
    sink.add(C.missingField, pointer('fighters'));
    return undefined;
  }
  if (!isPlainObject(value)) {
    sink.add(C.badFighters, pointer('fighters'));
    return undefined;
  }
  let valid = true;
  for (const key of Object.keys(value)) {
    if (!oneOf(SIDES, key)) {
      sink.add(C.badFighters, pointer('fighters', key));
      valid = false;
    }
  }
  const out: Partial<Record<Side, TimelineFighter>> = {};
  for (const side of SIDES) {
    const fighter = own(value, side);
    if (!isPlainObject(fighter)) {
      sink.add(C.badFighters, pointer('fighters', side));
      valid = false;
      continue;
    }
    unknownFields(sink, fighter, FIGHTER_FIELDS, 'fighters', side);
    const asset = own(fighter, 'asset');
    const maxHp = own(fighter, 'max_hp');
    if (typeof asset !== 'string' || !MANIFEST_LIMITS.idPattern.test(asset)) {
      sink.add(C.invalidValue, pointer('fighters', side, 'asset'));
      valid = false;
    } else if (manifests !== undefined && !manifests.has(asset)) {
      sink.add(C.unknownAsset, pointer('fighters', side, 'asset'), { asset });
      valid = false;
    }
    if (!isInt(maxHp) || maxHp < 1 || maxHp > L.maxHp) {
      sink.add(C.badMaxHp, pointer('fighters', side, 'max_hp'));
      valid = false;
    }
    if (typeof asset === 'string' && isInt(maxHp)) out[side] = { asset, max_hp: maxHp };
  }
  return valid && out.a && out.b ? { a: out.a, b: out.b } : undefined;
}

/** Structural parse of one cue; undefined (with errors) when it is malformed. */
function parseCue(sink: Sink, value: unknown, i: number): Cue | undefined {
  const at = (...rest: (string | number)[]) => pointer('cues', i, ...rest);
  if (!isPlainObject(value)) {
    sink.add(C.invalidType, at());
    return undefined;
  }
  const type = own(value, 'type');
  if (!oneOf(CUE_TYPES, type)) {
    sink.add(C.unknownCueType, at('type'));
    return undefined;
  }
  unknownFields(sink, value, CUE_FIELDS[type], 'cues', i);
  const start = sink.count;
  for (const key of CUE_FIELDS[type]) {
    if (!Object.hasOwn(value, key)) sink.add(C.missingField, at(key));
  }
  if (sink.count !== start) return undefined;

  const atMs = value.at_ms;
  if (!isInt(atMs) || atMs < 0 || atMs > L.maxDurationMs) sink.add(C.invalidType, at('at_ms'));

  if (type === 'strike') {
    const { actor, move, result, damage } = value;
    const targetHp = value.target_hp;
    if (!oneOf(SIDES, actor)) sink.add(C.invalidValue, at('actor'));
    if (!oneOf(MOVES, move)) sink.add(C.invalidValue, at('move'));
    if (!oneOf(STRIKE_RESULTS, result)) sink.add(C.invalidValue, at('result'));
    if (!isInt(damage) || damage < 0) sink.add(C.badDamage, at('damage'));
    if (!isInt(targetHp)) sink.add(C.invalidType, at('target_hp'));
    if (sink.count !== start) return undefined;
    return {
      at_ms: atMs as number,
      type,
      actor: actor as Side,
      move: move as StrikeCue['move'],
      result: result as StrikeCue['result'],
      damage: damage as number,
      target_hp: targetHp as number,
    };
  }

  if (type === 'intro') return sink.count === start ? { at_ms: atMs as number, type } : undefined;

  const fighter = value.fighter;
  if (!oneOf(SIDES, fighter)) sink.add(C.invalidValue, at('fighter'));
  if (type === 'knockdown') {
    const duration = value.duration_ms;
    if (!isInt(duration)) sink.add(C.invalidType, at('duration_ms'));
    if (sink.count !== start) return undefined;
    return {
      at_ms: atMs as number,
      type,
      fighter: fighter as Side,
      duration_ms: duration as number,
    };
  }
  if (sink.count !== start) return undefined;
  return { at_ms: atMs as number, type, fighter: fighter as Side };
}

function checkCues(
  sink: Sink,
  cues: readonly (Cue | undefined)[],
  fighters: Record<Side, TimelineFighter> | undefined,
  duration: number | undefined,
): void {
  const at = (i: number, ...rest: (string | number)[]) => pointer('cues', i, ...rest);
  const hp: Partial<Record<Side, number>> = fighters
    ? { a: fighters.a.max_hp, b: fighters.b.max_hp }
    : {};

  if (cues.length === 0) {
    sink.add(C.introFirst, pointer('cues'));
  }

  let prevAt = -1;
  let introAt: number | undefined;
  let lastStrike: { cue: StrikeCue; i: number } | undefined;
  let firstStrikeSeen = false;
  let knockedBy = -1;
  const knockdowns: { start: number; end: number }[] = [];
  let ko: { side: Side; at: number; i: number } | undefined;
  let win: { side: Side; at: number; i: number } | undefined;
  let koCount = 0;
  let winCount = 0;

  for (let i = 0; i < cues.length; i++) {
    const cue = cues[i];
    if (cue === undefined) continue;

    if (cue.at_ms < prevAt) sink.add(C.unsorted, at(i, 'at_ms'));
    prevAt = Math.max(prevAt, cue.at_ms);

    if (win !== undefined) sink.add(C.cueAfterWin, at(i));
    else if (ko !== undefined && cue.type !== 'win') sink.add(C.cueAfterKo, at(i));

    if (cue.type === 'intro') {
      if (i !== 0 || cue.at_ms !== 0) sink.add(C.introFirst, at(i));
      else introAt = cue.at_ms;
    } else if (i === 0) {
      sink.add(C.introFirst, at(i));
    }

    if (cue.type === 'strike') {
      if (!firstStrikeSeen) {
        firstStrikeSeen = true;
        if (introAt !== undefined && cue.at_ms < introAt + L.introGapMs) {
          sink.add(C.introGap, at(i, 'at_ms'));
        }
      }
      if (lastStrike !== undefined && cue.at_ms - lastStrike.cue.at_ms < L.strikeGapMs) {
        sink.add(C.strikeGap, at(i, 'at_ms'), { previous: lastStrike.i });
      }
      if (cue.result === 'dodge' && cue.damage !== 0) sink.add(C.dodgeDamage, at(i, 'damage'));
      for (const kd of knockdowns) {
        if (cue.at_ms > kd.start && cue.at_ms <= kd.end) {
          sink.add(C.strikeDuringKnockdown, at(i, 'at_ms'));
          break;
        }
      }
      const target = otherSide(cue.actor);
      const max = fighters?.[target].max_hp;
      if (max !== undefined && (cue.target_hp < 0 || cue.target_hp > max)) {
        sink.add(C.hpRange, at(i, 'target_hp'));
      } else {
        const before = hp[target];
        if (before !== undefined && cue.target_hp !== before - cue.damage) {
          sink.add(C.hpMismatch, at(i, 'target_hp'), { expected: before - cue.damage });
        }
        hp[target] = cue.target_hp;
      }
      if (cue.target_hp === 0) {
        const next = cues[i + 1];
        if (next === undefined || next.type !== 'ko' || next.fighter !== target) {
          sink.add(C.hpZeroWithoutKo, at(i, 'target_hp'));
        }
      }
      lastStrike = { cue, i };
    } else if (cue.type === 'knockdown') {
      if (cue.duration_ms < L.knockdownMinMs || cue.duration_ms > L.knockdownMaxMs) {
        sink.add(C.knockdownDuration, at(i, 'duration_ms'));
      }
      const s = lastStrike?.cue;
      const lag = s === undefined ? -1 : cue.at_ms - s.at_ms;
      if (lastStrike !== undefined && knockedBy === lastStrike.i) {
        // One knockdown per strike: stacked knockdowns would also unbound `cueWindow`.
        sink.add(C.knockdownDuplicate, at(i));
      } else if (
        s === undefined ||
        otherSide(s.actor) !== cue.fighter ||
        (s.result !== 'hit' && s.result !== 'crit') ||
        lag < 0 ||
        lag > L.knockdownLagMs
      ) {
        sink.add(C.knockdownWithoutStrike, at(i));
      }
      if (lastStrike !== undefined) knockedBy = lastStrike.i;
      knockdowns.push({ start: cue.at_ms, end: cue.at_ms + cue.duration_ms });
    } else if (cue.type === 'ko') {
      if (hp[cue.fighter] !== undefined && hp[cue.fighter] !== 0) {
        sink.add(C.koWithoutZeroHp, at(i));
      }
      koCount++;
      if (koCount > 1) sink.add(C.koCount, at(i));
      else ko = { side: cue.fighter, at: cue.at_ms, i };
    } else if (cue.type === 'win') {
      winCount++;
      if (winCount > 1) {
        sink.add(C.winCount, at(i));
      } else {
        win = { side: cue.fighter, at: cue.at_ms, i };
        if (ko === undefined) {
          sink.add(C.winGap, at(i));
        } else {
          if (cue.fighter === ko.side) sink.add(C.winSide, at(i, 'fighter'));
          if (cue.at_ms < ko.at + L.winGapMs) sink.add(C.winGap, at(i, 'at_ms'));
        }
      }
    }
  }

  if (koCount === 0) sink.add(C.koCount, pointer('cues'));
  if (winCount === 0) sink.add(C.winCount, pointer('cues'));
  if (win !== undefined && duration !== undefined && duration < win.at + L.winPoseMs) {
    sink.add(C.durationTooShort, pointer('duration_ms'));
  }
}

function run(sink: Sink, json: unknown, manifests: ManifestsById | undefined): Timeline | null {
  if (!isPlainObject(json)) {
    sink.add(C.notObject, '');
    return null;
  }
  const version = own(json, 'timeline_version');
  if (version === undefined) {
    sink.add(C.missingField, pointer('timeline_version'));
    return null;
  }
  if (version !== TIMELINE_VERSION) {
    sink.add(C.unsupportedVersion, pointer('timeline_version'));
    return null;
  }
  unknownFields(sink, json, ROOT_FIELDS);

  const durationValue = own(json, 'duration_ms');
  let duration: number | undefined;
  if (durationValue === undefined) {
    sink.add(C.missingField, pointer('duration_ms'));
  } else if (!isInt(durationValue) || durationValue < 1 || durationValue > L.maxDurationMs) {
    sink.add(C.badDuration, pointer('duration_ms'));
  } else {
    duration = durationValue;
  }

  const fighters = checkFighters(sink, own(json, 'fighters'), manifests);

  const cuesValue = own(json, 'cues');
  if (cuesValue === undefined) {
    sink.add(C.missingField, pointer('cues'));
    return null;
  }
  if (!Array.isArray(cuesValue)) {
    sink.add(C.invalidType, pointer('cues'));
    return null;
  }
  if (cuesValue.length > L.maxCues) {
    sink.add(C.tooManyCues, pointer('cues'), { count: cuesValue.length });
    return null;
  }
  const cues: (Cue | undefined)[] = [];
  for (let i = 0; i < cuesValue.length; i++) cues.push(parseCue(sink, cuesValue[i], i));
  checkCues(sink, cues, fighters, duration);

  if (sink.count > 0 || fighters === undefined || duration === undefined) return null;
  return {
    timeline_version: TIMELINE_VERSION,
    duration_ms: duration,
    fighters,
    cues: cues.filter((c): c is Cue => c !== undefined),
  };
}

/**
 * Validates an untrusted timeline. With `manifests`, every fighter asset must be among them
 * (`timeline.unknown_asset`).
 */
export function validateTimeline(json: unknown, manifests?: ManifestsById): TimelineResult {
  const sink: Sink = new ErrorSink<TimelineErrorCode>(L.maxErrors, C.tooManyErrors);
  let timeline: Timeline | null = null;
  try {
    timeline = run(sink, json, manifests);
  } catch (error) {
    if (!(error instanceof StopValidation)) throw error;
  }
  if (timeline !== null && sink.count === 0) return { ok: true, timeline };
  return { ok: false, errors: sink.errors };
}
