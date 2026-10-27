import { describe, expect, it } from 'vitest';
import { validateTimeline } from '@torneo/renderer/timeline';
import {
  ENGINE_LOGS,
  loadEngineLogTimeline,
  parseEngineLog,
} from '../src/lib/features/fight/engine-logs.js';

// 002 RF-C3 (plan-c decision 7): the `?log=` allow-list and its loader.
describe('parseEngineLog', () => {
  it('accepts only the exact names of the allow-list', () => {
    expect(parseEngineLog('ko-comeback')).toBe('ko-comeback');
    for (const raw of [null, '', 'KO-comeback', 'ko-comeback ', 'log-ko-comeback', 'toString']) {
      expect(parseEngineLog(raw), String(raw)).toBeNull();
    }
    expect(Object.keys(ENGINE_LOGS)).toEqual(['ko-comeback']);
  });
});

describe('loadEngineLogTimeline', () => {
  it('gives the timeline of the log with the assets of each side', async () => {
    const timeline = await loadEngineLogTimeline('ko-comeback', { a: 'dummy_b', b: 'dummy_a' });
    expect(timeline.fighters).toEqual({
      a: { asset: 'dummy_b', max_hp: 700 },
      b: { asset: 'dummy_a', max_hp: 860 },
    });
    expect(timeline.cues.find((c) => c.type === 'win')).toMatchObject({ fighter: 'b' });
    expect(validateTimeline(timeline).ok).toBe(true);
  });

  it('a log the adapter rejects throws with code timeline_invalid', async () => {
    const original = ENGINE_LOGS['ko-comeback'];
    const broken = { default: { ruleset_version: 1, a: {}, b: {}, events: [] } };
    (ENGINE_LOGS as Record<string, unknown>)['ko-comeback'] = async () => broken;
    try {
      await expect(loadEngineLogTimeline('ko-comeback', { a: 'x', b: 'y' })).rejects.toMatchObject({
        code: 'timeline_invalid',
      });
    } finally {
      (ENGINE_LOGS as Record<string, unknown>)['ko-comeback'] = original;
    }
  });
});
