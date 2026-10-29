// The ruleset golden written by crates/content (007 RF-A5): without rulesets there is no guide,
// so a missing or invalid golden is `io.golden_invalid` (exit 2).
import path from 'node:path';
import { parseRulesetTiming, type RulesetTiming } from '@torneo/renderer/guide';
import { readBounded } from './fsx.ts';
import { TOOL_LIMITS } from './limits.ts';

export const GOLDEN_FILE = 'crates/content/tests/golden/ruleset-timing.json';

export function loadGolden(root: string): readonly RulesetTiming[] | null {
  const read = readBounded(
    path.join(root, ...GOLDEN_FILE.split('/')),
    TOOL_LIMITS.jsonFileMaxBytes,
  );
  if (!read.ok) return null;
  let json: unknown;
  try {
    json = JSON.parse(read.bytes.toString('utf8'));
  } catch {
    return null;
  }
  const result = parseRulesetTiming(json);
  return result.ok ? result.rulesets : null;
}
