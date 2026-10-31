import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import decisionA from './fixtures/golden/decision-a.timeline.json' with { type: 'json' };
import decisionB from './fixtures/golden/decision-b.timeline.json' with { type: 'json' };
import draw from './fixtures/golden/draw.timeline.json' with { type: 'json' };
import spike from '../fixtures/spike.timeline.json' with { type: 'json' };
import { validateTimeline, type Cue, type Timeline } from '../src/timeline/index.ts';
import { applyOps, loadFixtures } from './support/patch.ts';
import { MANIFESTS } from './support/placeholders.ts';

// 010 RF46: the same rules without the two that ask for the closing, v2 only.
const CLOSING_ONLY: readonly string[] = ['timeline.ending_missing', 'timeline.closing_missing'];
const v2 = loadFixtures(fileURLToPath(new URL('./fixtures/timeline-v2/', import.meta.url)));
const v1 = loadFixtures(fileURLToPath(new URL('./fixtures/timeline/', import.meta.url)));
const PARTIAL = { partial: true } as const;

const errorsOf = (result: ReturnType<typeof validateTimeline>) => (result.ok ? [] : result.errors);

/** What the adapter gives for a cut by time: no closing and a provisional duration. */
function cut(timeline: Timeline, c: number): Timeline {
  return {
    ...timeline,
    duration_ms: 90_000,
    cues: timeline.cues.filter((q) => q.at_ms <= c && q.type !== 'win' && q.type !== 'draw'),
  };
}

const GOLDENS = [decisionA, decisionB, draw] as unknown as Timeline[];

describe('validateTimeline with partial (010 RF46)', () => {
  const allRules = [
    ...v2.rules,
    ...v1.rules.map((rule) => ({
      ...rule,
      // The v1 document as v2: a version rule would not say anything about the closing rules.
      ops: [{ op: 'set', path: '/timeline_version', value: 2 } as const, ...rule.ops],
    })),
  ];

  it.each(allRules.map((rule) => [rule.name, rule] as const))(
    '%s: partial gives the strict errors without the two closing codes',
    (name, rule) => {
      const base = v2.rules.includes(rule) ? v2.valid : v1.valid;
      const json = applyOps(base, rule.ops);
      const strict = errorsOf(validateTimeline(json, MANIFESTS));
      const partial = errorsOf(validateTimeline(json, MANIFESTS, PARTIAL));
      expect(partial, name).toEqual(strict.filter((e) => !CLOSING_ONLY.includes(e.code)));
    },
  );

  it('without time_up nor closing, or without the closing alone, only partial passes', () => {
    const noClosing = applyOps(v2.valid, [{ op: 'delete', path: '/cues/7' }]);
    const noEnding = applyOps(noClosing, [{ op: 'delete', path: '/cues/6' }]);
    const codes = (json: unknown) => errorsOf(validateTimeline(json, MANIFESTS)).map((e) => e.code);
    expect(codes(noClosing)).toEqual(['timeline.closing_missing']);
    expect(codes(noEnding)).toEqual(['timeline.ending_missing']);
    expect(validateTimeline(noClosing, MANIFESTS, PARTIAL).ok).toBe(true);
    expect(validateTimeline(noEnding, MANIFESTS, PARTIAL).ok).toBe(true);
  });

  it('every cut by time of the goldens passes, and strictly fails only for the closing', () => {
    for (const golden of GOLDENS) {
      for (const cue of golden.cues) {
        const prefix = cut(golden, cue.at_ms);
        expect(validateTimeline(prefix, MANIFESTS, PARTIAL)).toEqual({ ok: true, timeline: prefix });
        const codes = errorsOf(validateTimeline(prefix, MANIFESTS)).map((e) => e.code);
        expect(codes.length).toBe(1);
        expect(CLOSING_ONLY).toContain(codes[0]);
      }
    }
  });

  it('a complete timeline passes the same with and without partial', () => {
    for (const golden of GOLDENS) {
      expect(validateTimeline(golden, MANIFESTS, PARTIAL)).toEqual(
        validateTimeline(golden, MANIFESTS),
      );
    }
  });

  it('a closing that arrived is checked in full', () => {
    const golden = draw as unknown as Timeline;
    const short = { ...golden, duration_ms: golden.duration_ms - 2500 };
    expect(errorsOf(validateTimeline(short, MANIFESTS, PARTIAL)).map((e) => e.code)).toEqual([
      'timeline.duration_too_short',
    ]);
  });

  it('a strike that leaves HP 0 without its ko fails: a cut by time never splits them', () => {
    const cues: Cue[] = [
      { at_ms: 0, type: 'intro' },
      {
        at_ms: 3000,
        type: 'strike',
        actor: 'a',
        move: 'attack_light',
        result: 'hit',
        damage: 100,
        target_hp: 0,
      },
    ];
    const json = {
      timeline_version: 2,
      duration_ms: 90_000,
      fighters: {
        a: { asset: 'dummy_a', max_hp: 100 },
        b: { asset: 'dummy_b', max_hp: 100 },
      },
      cues,
    };
    expect(errorsOf(validateTimeline(json, MANIFESTS, PARTIAL)).map((e) => e.code)).toEqual([
      'timeline.hp_zero_without_ko',
    ]);
  });

  it('an empty list of cues still fails: the intro is required', () => {
    const json = { ...(v2.valid as object), cues: [] };
    expect(errorsOf(validateTimeline(json, MANIFESTS, PARTIAL)).map((e) => e.code)).toEqual([
      'timeline.intro_first',
    ]);
  });

  it('v1 does not admit partial', () => {
    expect(validateTimeline(spike, MANIFESTS, PARTIAL)).toEqual({
      ok: false,
      errors: [{ code: 'timeline.unsupported_version', path: '/timeline_version' }],
    });
    expect(validateTimeline(spike, MANIFESTS).ok).toBe(true);
  });

  it('partial: false is the strict validator', () => {
    const prefix = cut(GOLDENS[0] as Timeline, 5000);
    expect(validateTimeline(prefix, MANIFESTS, { partial: false })).toEqual(
      validateTimeline(prefix, MANIFESTS),
    );
  });
});
