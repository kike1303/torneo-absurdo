import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { TIMELINE_LIMITS } from '@torneo/renderer/timeline';
import { GOLDEN_DIR } from './support/golden-logs.js';

// 006 RF5: `engine::TIMELINE_CONTRACT` is a mirror of the renderer's rules. The engine writes it
// as a golden; this test ties every key to its pair of `TIMELINE_LIMITS`, and the key set is exact.
const PAIRS = {
  intro_gap_ms: 'introGapMs',
  strike_gap_ms: 'strikeGapMs',
  knockdown_min_ms: 'knockdownMinMs',
  knockdown_max_ms: 'knockdownMaxMs',
  win_gap_ms: 'winGapMs',
  win_pose_ms: 'winPoseMs',
} as const satisfies Record<string, keyof typeof TIMELINE_LIMITS>;

const contract = JSON.parse(readFileSync(`${GOLDEN_DIR}timeline-contract.json`, 'utf8')) as Record<
  string,
  unknown
>;

describe('timeline contract (006 RF5)', () => {
  it('has exactly the six keys of the mirror', () => {
    expect(Object.keys(contract).sort()).toEqual(Object.keys(PAIRS).sort());
  });

  it.each(Object.entries(PAIRS))('%s equals TIMELINE_LIMITS.%s', (key, limit) => {
    expect(contract[key]).toBe(TIMELINE_LIMITS[limit]);
  });
});
