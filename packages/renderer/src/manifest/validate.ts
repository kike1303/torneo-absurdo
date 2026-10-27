// validateManifest (RF-A4, RF-A5, plan-a1 decision 14). Pure; the input is untrusted.
import { MANIFEST_CODES as C } from './codes.ts';
import {
  ErrorSink,
  StopValidation,
  isFiniteNumber,
  isInt,
  isPlainObject,
  own,
  pointer,
} from './input.ts';
import {
  ATTACK_ANIMATIONS,
  LIMITS,
  LOOPING_ANIMATIONS,
  MANIFEST_FORMAT,
  MANIFEST_FORMAT_VERSION,
  NON_LOOPING_ANIMATIONS,
  OPTIONAL_ANIMATIONS,
  REQUIRED_ANIMATIONS,
  type AnimationCode,
  type ManifestError,
  type ManifestErrorCode,
  type SpriteAnimation,
  type SpriteFrame,
  type SpriteManifest,
  type SpriteSheet,
} from './types.ts';

export type ManifestResult =
  | { readonly ok: true; readonly manifest: SpriteManifest }
  | { readonly ok: false; readonly errors: readonly ManifestError[] };

export interface ValidateManifestOptions {
  /** The folder name; `id` must equal it (`manifest.id_mismatch`). */
  readonly expectedId?: string;
}

const ROOT_FIELDS = [
  'format',
  'format_version',
  'id',
  'pixel_art',
  'scale',
  'sheets',
  'frames',
  'animations',
];
const SHEET_FIELDS = ['file', 'width', 'height'];
const FRAME_FIELDS = ['sheet', 'x', 'y', 'w', 'h', 'pivot_x', 'pivot_y'];
const ANIMATION_FIELDS = ['frames', 'frame_ms', 'loop', 'impact_frame'];
const KNOWN_ANIMATIONS: readonly string[] = [...REQUIRED_ANIMATIONS, ...OPTIONAL_ANIMATIONS];
const ATTACKS: readonly string[] = ATTACK_ANIMATIONS;

type Sink = ErrorSink<ManifestErrorCode>;

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

/** Reads a required field; reports `missing_field` and returns undefined when absent. */
function required(
  sink: Sink,
  obj: Record<string, unknown>,
  key: string,
  ...path: (string | number)[]
): unknown {
  if (!Object.hasOwn(obj, key)) {
    sink.add(C.missingField, pointer(...path, key));
    return undefined;
  }
  return obj[key];
}

function checkSheets(sink: Sink, value: unknown): SpriteSheet[] | undefined {
  if (value === undefined) return undefined;
  if (!Array.isArray(value)) {
    sink.add(C.invalidType, pointer('sheets'));
    return undefined;
  }
  if (value.length < LIMITS.sheetsMin || value.length > LIMITS.sheetsMax) {
    sink.add(C.sheetsCount, pointer('sheets'), { count: value.length });
    return undefined;
  }
  const sheets: SpriteSheet[] = [];
  let valid = true;
  for (let i = 0; i < value.length; i++) {
    const sheet: unknown = value[i];
    if (!isPlainObject(sheet)) {
      sink.add(C.invalidType, pointer('sheets', i));
      valid = false;
      continue;
    }
    unknownFields(sink, sheet, SHEET_FIELDS, 'sheets', i);
    const file = required(sink, sheet, 'file', 'sheets', i);
    const width = required(sink, sheet, 'width', 'sheets', i);
    const height = required(sink, sheet, 'height', 'sheets', i);
    if (file !== undefined && (typeof file !== 'string' || !LIMITS.sheetFilePattern.test(file))) {
      sink.add(C.sheetFile, pointer('sheets', i, 'file'));
      valid = false;
    }
    for (const [key, side] of [
      ['width', width],
      ['height', height],
    ] as const) {
      if (side !== undefined && (!isInt(side) || side < 1 || side > LIMITS.sheetSideMax)) {
        sink.add(C.sheetSize, pointer('sheets', i, key));
        valid = false;
      }
    }
    if (typeof file === 'string' && isInt(width) && isInt(height)) {
      sheets.push({ file, width, height });
    } else {
      valid = false;
    }
  }
  return valid ? sheets : undefined;
}

function checkFrames(
  sink: Sink,
  value: unknown,
  sheets: readonly SpriteSheet[] | undefined,
): SpriteFrame[] | undefined {
  if (value === undefined) return undefined;
  if (!Array.isArray(value)) {
    sink.add(C.invalidType, pointer('frames'));
    return undefined;
  }
  if (value.length < LIMITS.framesMin || value.length > LIMITS.framesMax) {
    sink.add(C.framesCount, pointer('frames'), { count: value.length });
    return undefined;
  }
  const frames: SpriteFrame[] = [];
  let valid = true;
  for (let i = 0; i < value.length; i++) {
    const frame: unknown = value[i];
    if (!isPlainObject(frame)) {
      sink.add(C.invalidType, pointer('frames', i));
      valid = false;
      continue;
    }
    unknownFields(sink, frame, FRAME_FIELDS, 'frames', i);
    const nums: Record<string, number> = {};
    let complete = true;
    for (const key of FRAME_FIELDS) {
      const v = required(sink, frame, key, 'frames', i);
      if (v === undefined) {
        complete = false;
      } else if (!isInt(v)) {
        sink.add(C.invalidType, pointer('frames', i, key));
        complete = false;
      } else {
        nums[key] = v;
      }
    }
    if (!complete) {
      valid = false;
      continue;
    }
    const f: SpriteFrame = {
      sheet: nums.sheet ?? 0,
      x: nums.x ?? 0,
      y: nums.y ?? 0,
      w: nums.w ?? 0,
      h: nums.h ?? 0,
      pivot_x: nums.pivot_x ?? 0,
      pivot_y: nums.pivot_y ?? 0,
    };
    const sheet = sheets?.[f.sheet];
    if (f.sheet < 0 || (sheets !== undefined && sheet === undefined)) {
      sink.add(C.frameSheet, pointer('frames', i, 'sheet'));
      valid = false;
    }
    if (f.w < 1 || f.h < 1) {
      sink.add(C.frameSize, pointer('frames', i));
      valid = false;
    } else {
      if (
        sheet !== undefined &&
        (f.x < 0 || f.y < 0 || f.x + f.w > sheet.width || f.y + f.h > sheet.height)
      ) {
        sink.add(C.frameOutOfSheet, pointer('frames', i));
        valid = false;
      }
      if (f.pivot_x < 0 || f.pivot_y < 0 || f.pivot_x > f.w || f.pivot_y > f.h) {
        sink.add(C.pivotOutOfFrame, pointer('frames', i));
        valid = false;
      }
    }
    frames.push(f);
  }
  return valid ? frames : undefined;
}

function checkAnimation(
  sink: Sink,
  code: AnimationCode,
  value: unknown,
  frameCount: number | undefined,
): SpriteAnimation | undefined {
  const at = (...rest: (string | number)[]) => pointer('animations', code, ...rest);
  if (!isPlainObject(value)) {
    sink.add(C.invalidType, at());
    return undefined;
  }
  unknownFields(sink, value, ANIMATION_FIELDS, 'animations', code);
  const start = sink.count;

  const frames = required(sink, value, 'frames', 'animations', code);
  const frameMs = required(sink, value, 'frame_ms', 'animations', code);
  const loop = required(sink, value, 'loop', 'animations', code);
  const impact = own(value, 'impact_frame');

  let indices: number[] | undefined;
  if (frames !== undefined) {
    if (!Array.isArray(frames)) {
      sink.add(C.invalidType, at('frames'));
    } else if (
      frames.length < LIMITS.animationFramesMin ||
      frames.length > LIMITS.animationFramesMax
    ) {
      sink.add(C.animationFramesCount, at('frames'), { count: frames.length });
    } else {
      indices = [];
      for (let i = 0; i < frames.length; i++) {
        const idx: unknown = frames[i];
        if (!isInt(idx) || idx < 0 || (frameCount !== undefined && idx >= frameCount)) {
          sink.add(C.animationFrameIndex, at('frames', i));
        } else {
          indices.push(idx);
        }
      }
    }
  }

  let durations: number[] | undefined;
  if (frameMs !== undefined) {
    if (!Array.isArray(frameMs)) {
      sink.add(C.invalidType, at('frame_ms'));
    } else if (Array.isArray(frames) && frameMs.length !== frames.length) {
      sink.add(C.frameMsLength, at('frame_ms'), {
        expected: frames.length,
        count: frameMs.length,
      });
    } else if (frameMs.length <= LIMITS.animationFramesMax) {
      durations = [];
      for (let i = 0; i < frameMs.length; i++) {
        const ms: unknown = frameMs[i];
        if (!isInt(ms) || ms < LIMITS.frameMsMin || ms > LIMITS.frameMsMax) {
          sink.add(C.frameMsRange, at('frame_ms', i));
        } else {
          durations.push(ms);
        }
      }
    }
  }

  if (loop !== undefined && typeof loop !== 'boolean') sink.add(C.invalidType, at('loop'));
  if (loop === false && LOOPING_ANIMATIONS.includes(code)) sink.add(C.loopRequired, at('loop'));
  if (loop === true && NON_LOOPING_ANIMATIONS.includes(code)) sink.add(C.loopForbidden, at('loop'));

  const total = durations?.reduce((sum, ms) => sum + ms, 0);
  if (loop === false && total !== undefined && total > LIMITS.nonLoopingTotalMaxMs) {
    sink.add(C.animationTooLong, at('frame_ms'), { total_ms: total });
  }

  const isAttack = ATTACKS.includes(code);
  if (impact === undefined) {
    if (isAttack) sink.add(C.impactFrameMissing, at('impact_frame'));
  } else if (!isAttack) {
    sink.add(C.impactFrameForbidden, at('impact_frame'));
  } else if (!isInt(impact) || impact < 0 || (Array.isArray(frames) && impact >= frames.length)) {
    sink.add(C.impactFrameRange, at('impact_frame'));
  } else if (durations !== undefined) {
    const windup = durations.slice(0, impact).reduce((sum, ms) => sum + ms, 0);
    if (windup > LIMITS.windupMaxMs) {
      sink.add(C.windupTooLong, at('impact_frame'), { windup_ms: windup });
    }
  }

  if (sink.count !== start || indices === undefined || durations === undefined) return undefined;
  if (typeof loop !== 'boolean') return undefined;
  return isInt(impact)
    ? { frames: indices, frame_ms: durations, loop, impact_frame: impact }
    : { frames: indices, frame_ms: durations, loop };
}

function checkAnimations(
  sink: Sink,
  value: unknown,
  frameCount: number | undefined,
): Partial<Record<AnimationCode, SpriteAnimation>> | undefined {
  if (value === undefined) return undefined;
  if (!isPlainObject(value)) {
    sink.add(C.invalidType, pointer('animations'));
    return undefined;
  }
  // A Map, so a key such as `__proto__` can never touch a prototype.
  const result = new Map<AnimationCode, SpriteAnimation>();
  for (const key of Object.keys(value)) {
    if (!KNOWN_ANIMATIONS.includes(key)) {
      sink.add(C.unknownAnimation, pointer('animations', key));
      continue;
    }
    const code = key as AnimationCode;
    const animation = checkAnimation(sink, code, value[key], frameCount);
    if (animation !== undefined) result.set(code, animation);
  }
  for (const code of REQUIRED_ANIMATIONS) {
    if (!Object.hasOwn(value, code)) sink.add(C.missingAnimation, pointer('animations', code));
  }
  const out: Partial<Record<AnimationCode, SpriteAnimation>> = {};
  for (const code of KNOWN_ANIMATIONS as readonly AnimationCode[]) {
    const animation = result.get(code);
    if (animation !== undefined) out[code] = animation;
  }
  return out;
}

function run(sink: Sink, json: unknown, options: ValidateManifestOptions): SpriteManifest | null {
  if (!isPlainObject(json)) {
    sink.add(C.notObject, '');
    return null;
  }
  const version = own(json, 'format_version');
  if (version === undefined) {
    sink.add(C.missingField, pointer('format_version'));
    return null;
  }
  if (version !== MANIFEST_FORMAT_VERSION) {
    sink.add(C.unsupportedVersion, pointer('format_version'));
    return null;
  }
  unknownFields(sink, json, ROOT_FIELDS);

  const format = required(sink, json, 'format');
  if (format !== undefined && format !== MANIFEST_FORMAT) sink.add(C.badFormat, pointer('format'));

  const id = required(sink, json, 'id');
  if (id !== undefined) {
    if (typeof id !== 'string' || !LIMITS.idPattern.test(id)) {
      sink.add(C.badId, pointer('id'));
    } else if (options.expectedId !== undefined && id !== options.expectedId) {
      sink.add(C.idMismatch, pointer('id'), { expected: options.expectedId });
    }
  }

  const pixelArt = required(sink, json, 'pixel_art');
  if (pixelArt !== undefined && typeof pixelArt !== 'boolean') {
    sink.add(C.invalidType, pointer('pixel_art'));
  }

  const scale = own(json, 'scale');
  if (
    scale !== undefined &&
    (!isFiniteNumber(scale) || scale < LIMITS.scaleMin || scale > LIMITS.scaleMax)
  ) {
    sink.add(C.badScale, pointer('scale'));
  }

  const sheets = checkSheets(sink, required(sink, json, 'sheets'));
  const framesValue = required(sink, json, 'frames');
  const frames = checkFrames(sink, framesValue, sheets);
  const frameCount = Array.isArray(framesValue) ? framesValue.length : undefined;
  const animations = checkAnimations(sink, required(sink, json, 'animations'), frameCount);

  if (sink.count > 0 || !sheets || !frames || !animations) return null;
  if (typeof id !== 'string' || typeof pixelArt !== 'boolean') return null;
  const base = {
    format: MANIFEST_FORMAT,
    format_version: MANIFEST_FORMAT_VERSION,
    id,
    pixel_art: pixelArt,
    sheets,
    frames,
    animations: animations as SpriteManifest['animations'],
  } as const;
  return isFiniteNumber(scale) ? { ...base, scale } : base;
}

export function validateManifest(
  json: unknown,
  options: ValidateManifestOptions = {},
): ManifestResult {
  const sink: Sink = new ErrorSink<ManifestErrorCode>(LIMITS.maxErrors, C.tooManyErrors);
  let manifest: SpriteManifest | null = null;
  try {
    manifest = run(sink, json, options);
  } catch (error) {
    if (!(error instanceof StopValidation)) throw error;
  }
  if (manifest !== null && sink.count === 0) return { ok: true, manifest };
  return { ok: false, errors: sink.errors };
}
