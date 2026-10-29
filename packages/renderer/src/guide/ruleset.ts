// The ruleset timings the guide reads, from the golden that crates/content writes
// (crates/content/tests/golden/ruleset-timing.json, 007 RF-A5). The input is untrusted.
import { ErrorSink, StopValidation, isPlainObject, own, pointer } from '../manifest/input.ts';
import { LIMITS } from '../manifest/types.ts';
import { RULESET_CODES } from './codes.ts';

/** One ruleset version, in snake_case like the golden. */
export interface RulesetTiming {
  readonly ruleset_version: number;
  readonly windup_strike_ms: number;
  readonly windup_heavy_ms: number;
  readonly windup_special_ms: number;
  readonly knockdown_ms: number;
}

export interface RulesetError {
  readonly code: typeof RULESET_CODES.invalid;
  readonly path: string;
}

export type RulesetResult =
  | { readonly ok: true; readonly rulesets: readonly RulesetTiming[] }
  | { readonly ok: false; readonly errors: readonly RulesetError[] };

const ROOT_KEYS: readonly string[] = ['versions'];
const TIMING_KEYS = [
  'ruleset_version',
  'windup_strike_ms',
  'windup_heavy_ms',
  'windup_special_ms',
  'knockdown_ms',
] as const;

type Sink = ErrorSink<RulesetError['code']>;

function isCount(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}

function parseTiming(sink: Sink, value: unknown, i: number): RulesetTiming | undefined {
  if (!isPlainObject(value)) {
    sink.add(RULESET_CODES.invalid, pointer('versions', i));
    return undefined;
  }
  const keys: readonly string[] = TIMING_KEYS;
  let valid = true;
  for (const key of Object.keys(value)) {
    if (!keys.includes(key)) {
      sink.add(RULESET_CODES.invalid, pointer('versions', i, key));
      valid = false;
    }
  }
  const out: Record<string, number> = {};
  for (const key of TIMING_KEYS) {
    const field = own(value, key);
    if (!isCount(field)) {
      sink.add(RULESET_CODES.invalid, pointer('versions', i, key));
      valid = false;
    } else {
      out[key] = field;
    }
  }
  return valid ? (out as unknown as RulesetTiming) : undefined;
}

/**
 * Exact keys, safe integers ≥ 0, at least one version and `ruleset_version` strictly increasing
 * (unique and sorted). Every error is `ruleset.invalid` with the JSON Pointer of the problem; the
 * cut at `LIMITS.maxErrors` is one more `ruleset.invalid` at the root.
 */
export function parseRulesetTiming(json: unknown): RulesetResult {
  const sink: Sink = new ErrorSink(LIMITS.maxErrors, RULESET_CODES.invalid);
  const rulesets: RulesetTiming[] = [];
  try {
    if (!isPlainObject(json)) {
      sink.add(RULESET_CODES.invalid, '');
    } else {
      for (const key of Object.keys(json)) {
        if (!ROOT_KEYS.includes(key)) sink.add(RULESET_CODES.invalid, pointer(key));
      }
      const versions = own(json, 'versions');
      if (!Array.isArray(versions) || versions.length === 0) {
        sink.add(RULESET_CODES.invalid, pointer('versions'));
      } else {
        let previous: number | undefined;
        for (let i = 0; i < versions.length; i++) {
          const timing = parseTiming(sink, versions[i], i);
          const version = isPlainObject(versions[i])
            ? own(versions[i] as Record<string, unknown>, 'ruleset_version')
            : undefined;
          if (isCount(version)) {
            if (previous !== undefined && version <= previous) {
              sink.add(RULESET_CODES.invalid, pointer('versions', i, 'ruleset_version'));
            }
            previous = version;
          }
          if (timing !== undefined) rulesets.push(timing);
        }
      }
    }
  } catch (error) {
    if (!(error instanceof StopValidation)) throw error;
  }
  if (sink.count > 0) return { ok: false, errors: sink.errors as RulesetError[] };
  return { ok: true, rulesets };
}
