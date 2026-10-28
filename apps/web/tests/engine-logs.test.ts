import { describe, expect, it } from 'vitest';
import { validateTimeline } from '@torneo/renderer/timeline';
import {
  ENGINE_LOGS,
  loadEngineLogTimeline,
  parseEngineLog,
} from '../src/lib/features/fight/engine-logs.js';
import comeback from '../src/lib/features/fight/fixtures/log-ko-comeback.json';

// 002 RF-C3 (plan-c decision 7), 005 RF15 and 006 RF10: the `?log=` allow-list and its loader. The
// figures of a log come from its copy, never from literals (006 plan, decision 9).
describe('parseEngineLog', () => {
  it('accepts only the exact names of the allow-list', () => {
    const names = [
      'ko-comeback',
      'ko-clutch',
      'ko-decisive-crit',
      'ko-knockdown',
      'decision-a',
      'decision-b',
      'draw',
    ];
    for (const name of names) expect(parseEngineLog(name)).toBe(name);
    for (const raw of [
      null,
      '',
      'KO-comeback',
      'ko-comeback ',
      'log-ko-comeback',
      'toString',
      'Draw',
      'log-draw',
      'decision',
      'ko-Knockdown',
      'log-ko-clutch',
      'ko',
    ]) {
      expect(parseEngineLog(raw), String(raw)).toBeNull();
    }
    expect(Object.keys(ENGINE_LOGS)).toEqual(names);
  });
});

describe('loadEngineLogTimeline', () => {
  it('gives the timeline of the log with the assets of each side', async () => {
    const timeline = await loadEngineLogTimeline('ko-comeback', { a: 'dummy_b', b: 'dummy_a' });
    const end = comeback.events.at(-1);
    expect(end?.type).toBe('end');
    expect(timeline.fighters).toEqual({
      a: { asset: 'dummy_b', max_hp: comeback.a.max_hp },
      b: { asset: 'dummy_a', max_hp: comeback.b.max_hp },
    });
    expect(timeline.cues.find((c) => c.type === 'win')).toMatchObject({
      fighter: end && 'winner' in end ? end.winner : undefined,
    });
    expect(validateTimeline(timeline).ok).toBe(true);
  });

  it('gives the draw: time_up and draw, no win', async () => {
    const timeline = await loadEngineLogTimeline('draw', { a: 'dummy_a', b: 'dummy_b' });
    expect(timeline.timeline_version).toBe(2);
    expect(timeline.cues.slice(-2)).toEqual([
      { at_ms: 86000, type: 'time_up' },
      { at_ms: 87500, type: 'draw' },
    ]);
    expect(timeline.cues.some((c) => c.type === 'win')).toBe(false);
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
