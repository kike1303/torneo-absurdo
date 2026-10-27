import { describe, expect, it } from 'vitest';
import {
  K,
  LOOKAHEAD_MS,
  LOOKBACK_MS,
  MARK_X,
  cueWindow,
  prepareScene,
  sceneAt,
  sceneAtTimeline,
  type PreparedScene,
} from '../src/scene/index.ts';
import { validateTimeline, type Side, type StrikeCue } from '../src/timeline/index.ts';
import { MANIFESTS } from './support/placeholders.ts';
import {
  goldenScene,
  shuffled,
  spikeScene,
  spikeTimeline,
  spread,
  syntheticTimeline,
  times,
} from './support/scenes.ts';
import { timeline as timeline_ } from './support/timeline-builder.ts';
import { WINDOW_MS, build, maxWindow, sequences } from './support/window-bound.ts';

// RF-A9 and plan-a1 decisions 2 and 16.
const timeline = spikeTimeline();
const prepared = spikeScene();
const OPTS = { reducedMotion: false };
const strikes = timeline.cues.filter((c): c is StrikeCue => c.type === 'strike');
const ko = timeline.cues.find((c) => c.type === 'ko');
const win = timeline.cues.find((c) => c.type === 'win');
const firstStrike = strikes[0]?.at_ms ?? 0;
const SAMPLES = spread(timeline.duration_ms, 200);

describe('sceneAt is a pure function of time', () => {
  it('gives deeply equal results for the same arguments', () => {
    for (const t of [0, 3200, 26_100, 57_600, 64_000]) {
      expect(sceneAt(prepared, t, OPTS)).toEqual(sceneAt(prepared, t, OPTS));
    }
  });

  it('matches the spec signature (sceneAtTimeline)', () => {
    expect(sceneAtTimeline(timeline, MANIFESTS, 26_100, OPTS)).toEqual(
      sceneAt(prepared, 26_100, OPTS),
    );
  });

  it('does not depend on the order of evaluation (200 times)', () => {
    const forward = SAMPLES.map((t) => sceneAt(prepared, t, OPTS));
    const backward = [...SAMPLES]
      .reverse()
      .map((t) => sceneAt(prepared, t, OPTS))
      .reverse();
    const order = shuffled(
      SAMPLES.map((t, i) => [t, i] as const),
      7,
    );
    const random: unknown[] = [];
    for (const [t, i] of order) random[i] = sceneAt(prepared, t, OPTS);
    expect(backward).toEqual(forward);
    expect(random).toEqual(forward);
  });

  it('is prepared once and deeply frozen', () => {
    expect(Object.isFrozen(prepared)).toBe(true);
    expect(Object.isFrozen(prepared.cues)).toBe(true);
    expect(Object.isFrozen(prepared.cues[1]?.segments[0])).toBe(true);
    expect(Object.isFrozen(prepared.fighters.a.timing.idle.ends)).toBe(true);
  });
});

describe('HUD values', () => {
  const lastHp = (side: Side, t: number): number => {
    const hit = strikes.filter((s) => s.at_ms <= t && s.actor !== side).at(-1);
    return hit?.target_hp ?? timeline.fighters[side].max_hp;
  };

  it('shows the target_hp of the last strike with at_ms ≤ t', () => {
    for (const t of [...SAMPLES, ...strikes.map((s) => s.at_ms)]) {
      const scene = sceneAt(prepared, t, OPTS);
      expect(scene.hud.a.hp).toBe(lastHp('a', t));
      expect(scene.hud.b.hp).toBe(lastHp('b', t));
    }
  });

  it('keeps the trail ≥ HP, equal to it 800 ms after the last strike on that side', () => {
    for (const t of times(0, timeline.duration_ms, 16)) {
      const scene = sceneAt(prepared, t, OPTS);
      for (const side of ['a', 'b'] as const) {
        const { hp, trail } = scene.hud[side];
        expect(trail).toBeGreaterThanOrEqual(hp);
        const last = strikes.filter((s) => s.at_ms <= t && s.actor !== side).at(-1);
        if (last === undefined || t - last.at_ms > 800) expect(trail).toBe(hp);
      }
    }
  });
});

describe('fighters, banners and phase', () => {
  it('stands both fighters on their marks in idle, with b mirrored', () => {
    const scene = sceneAt(prepared, 2000, OPTS);
    expect(scene.fighters.a).toMatchObject({ animation: 'idle', x: 640 / 3, mirror: false });
    expect(scene.fighters.b).toMatchObject({ animation: 'idle', x: 1280 / 3, mirror: true });
    expect(MARK_X).toEqual({ a: 640 / 3, b: 1280 / 3 });
  });

  it('plays intro first', () => {
    expect(sceneAt(prepared, 100, OPTS).fighters.a.animation).toBe('intro');
  });

  it('shows the fight banner only in the intro and the ko banner from the KO on', () => {
    for (const t of times(0, timeline.duration_ms, 50)) {
      const codes = sceneAt(prepared, t, OPTS).banners.map((b) => b.code);
      if (codes.includes('fight')) expect(t).toBeLessThan(firstStrike);
      expect(codes.includes('ko')).toBe(t >= (ko?.at_ms ?? Infinity));
    }
    expect(sceneAt(prepared, firstStrike - 600, OPTS).banners.map((b) => b.code)).toEqual([
      'fight',
    ]);
  });

  it('reports the phase and the winner', () => {
    const phase = (t: number) => sceneAt(prepared, t, OPTS);
    expect(phase(0)).toMatchObject({ phase: 'intro', winner: null });
    expect(phase(firstStrike)).toMatchObject({ phase: 'fight', winner: null });
    expect(phase(ko?.at_ms ?? 0)).toMatchObject({ phase: 'ko', winner: null });
    expect(phase(win?.at_ms ?? 0)).toMatchObject({ phase: 'win', winner: 'a' });
    expect(phase(timeline.duration_ms)).toMatchObject({ phase: 'finished', winner: 'a' });
  });

  it('holds ko and win until the end', () => {
    const end = sceneAt(prepared, timeline.duration_ms + 5000, OPTS);
    expect(end.fighters.b.animation).toBe('ko');
    expect(end.fighters.a.animation).toBe('win');
  });
});

describe('bounded cost without a clock', () => {
  const check = (p: PreparedScene, duration: number) => {
    let max = 0;
    for (const t of times(0, duration, 16)) max = Math.max(max, cueWindow(p, t).length);
    return max;
  };

  it('reads at most K cues per frame on the spike script', () => {
    expect(check(prepared, timeline.duration_ms)).toBeLessThanOrEqual(K);
  });

  it('reads at most K cues per frame on a synthetic timeline of 1 000 cues', () => {
    const synthetic = syntheticTimeline();
    expect(synthetic.cues).toHaveLength(1000);
    const p = prepareScene(synthetic, MANIFESTS);
    expect(check(p, synthetic.duration_ms)).toBeLessThanOrEqual(K);
  });

  it('the window includes the windup look-ahead', () => {
    const next = strikes[5];
    if (next === undefined) throw new Error('no strike');
    const ids = cueWindow(prepared, next.at_ms - LOOKAHEAD_MS).map((c) => c.cue.at_ms);
    expect(ids).toContain(next.at_ms);
    expect(LOOKBACK_MS).toBe(3300);
  });

  it('every segment lies within the reach of the window around its cue', () => {
    for (const p of [prepared, prepareScene(syntheticTimeline(), MANIFESTS)]) {
      for (const pc of p.cues) {
        for (const segment of pc.segments) {
          expect(segment.owner).toBe(pc.index);
          expect(segment.start).toBeGreaterThanOrEqual(pc.cue.at_ms - LOOKAHEAD_MS);
          expect(segment.end).toBeLessThanOrEqual(pc.cue.at_ms + LOOKBACK_MS);
        }
      }
    }
  });
});

describe('QA findings', () => {
  it('clamps a non-finite t', () => {
    expect(sceneAt(prepared, Number.NaN, OPTS)).toEqual(sceneAt(prepared, 0, OPTS));
    expect(sceneAt(prepared, Number.NEGATIVE_INFINITY, OPTS)).toEqual(sceneAt(prepared, 0, OPTS));
    expect(sceneAt(prepared, Number.POSITIVE_INFINITY, OPTS)).toEqual(
      sceneAt(prepared, timeline.duration_ms, OPTS),
    );
  });

  it('does not freeze the caller manifests', () => {
    const manifest = MANIFESTS.get('dummy_a');
    prepareScene(timeline, MANIFESTS);
    expect(Object.isFrozen(manifest?.animations.idle.frames)).toBe(false);
    expect(Object.isFrozen(manifest?.frames)).toBe(false);
  });

  it('keeps K with a knockdown after every strike (the densest valid case)', () => {
    const b = timeline_({ maxHp: 1_000_000 });
    let at = 2000;
    for (let i = 0; i < 200; i++) {
      const actor = i % 2 === 0 ? 'a' : 'b';
      b.strike(at, actor, 'attack_light', 'hit', 1).knockdown(at, actor === 'a' ? 'b' : 'a', 800);
      at += 801;
    }
    const tl = b.finish(at);
    const p = prepareScene(tl, MANIFESTS);
    let max = 0;
    for (let t = 0; t <= tl.duration_ms; t += 16) max = Math.max(max, cueWindow(p, t).length);
    expect(max).toBeLessThanOrEqual(K);
  });
});

describe('the bound of cueWindow is 11 (plan-a2 decision 16a, 005 RF9)', () => {
  it('reaches 10 with 6 strikes, 4 of them with a knockdown, on a valid timeline', () => {
    // 3 × 801 + 2 × 600 = 3 603 ms ≤ 3 700 ms, the last strike with its knockdown.
    const tl = build(['K', 'S', 'K', 'S', 'K', 'K'], false);
    expect(validateTimeline(tl, MANIFESTS).ok).toBe(true);
    expect(maxWindow(tl)).toBe(10);
    expect(maxWindow(tl)).toBeLessThanOrEqual(K);
  });

  it('reaches 11 with a time_up at the last of those strikes', () => {
    const tl = build(['K', 'S', 'K', 'S', 'K', 'K', 'T'], false);
    expect(tl.timeline_version).toBe(2);
    expect(maxWindow(tl)).toBe(11);
    expect(maxWindow(tl)).toBeLessThanOrEqual(K);
  });

  it('no sequence of minimal gaps passes 11, and 11 is reached', () => {
    // Larger gaps than the minimum can only take cues out of a window, so the minimal steps
    // (strike alone, strike with knockdown, closing strike with ko and win, time_up at the last
    // strike and its closing) cover every case.
    expect(WINDOW_MS).toBe(3700);
    let max = 0;
    let count = 0;
    for (const steps of sequences()) {
      for (const intro of [false, true]) {
        const tl = build(steps, intro);
        expect(validateTimeline(tl, MANIFESTS).ok).toBe(true);
        const m = maxWindow(tl);
        expect(m, steps.join('')).toBeLessThanOrEqual(11);
        max = Math.max(max, m);
        count++;
      }
    }
    expect(count).toBeGreaterThan(100);
    expect(max).toBe(11);
    expect(max).toBeLessThanOrEqual(K);
  });
});

describe('sceneAt clamps t to the duration (plan-a2 decision 16b)', () => {
  it('a finite t past the end is the end', () => {
    expect(sceneAt(prepared, timeline.duration_ms + 5000, OPTS)).toEqual(
      sceneAt(prepared, timeline.duration_ms, OPTS),
    );
    expect(sceneAt(prepared, timeline.duration_ms + 5000, OPTS).t).toBe(timeline.duration_ms);
  });
});

describe('endings by time (005 RF6, RF7)', () => {
  const byTime = (winner: Side | null) => {
    const b = timeline_()
      .strike(3000, 'a', 'attack_light', 'hit', 100)
      .strike(4000, 'b', 'attack_light', 'hit', 50);
    return prepareScene(b.finishByTime(5000, winner), MANIFESTS);
  };
  // time_up at 5000, closing at 6500, end at 9000.
  const at = (p: PreparedScene, t: number, reducedMotion = false) =>
    sceneAt(p, t, { reducedMotion });

  it('reports phase, winner and ending at each closing instant of a decision', () => {
    const p = byTime('a');
    expect(at(p, 4999)).toMatchObject({ phase: 'fight', winner: null, ending: null });
    expect(at(p, 5000)).toMatchObject({ phase: 'time_up', winner: null, ending: null });
    expect(at(p, 6499)).toMatchObject({ phase: 'time_up', winner: null, ending: null });
    expect(at(p, 6500)).toMatchObject({ phase: 'win', winner: 'a', ending: 'decision' });
    expect(at(p, 9000)).toMatchObject({ phase: 'finished', winner: 'a', ending: 'decision' });
  });

  it('reports phase, winner and ending at each closing instant of a draw', () => {
    const p = byTime(null);
    expect(at(p, 5000)).toMatchObject({ phase: 'time_up', winner: null, ending: null });
    expect(at(p, 6500)).toMatchObject({ phase: 'draw', winner: null, ending: 'draw' });
    expect(at(p, 9000)).toMatchObject({ phase: 'finished', winner: null, ending: 'draw' });
  });

  it('reports the ko ending from the win of a KO on', () => {
    const p = prepareScene(timeline_({ version: 2 }).finish(3000, 'b'), MANIFESTS);
    expect(at(p, 3000)).toMatchObject({ phase: 'ko', ending: null });
    expect(at(p, 4500)).toMatchObject({ phase: 'win', winner: 'b', ending: 'ko' });
    expect(at(p, p.durationMs)).toMatchObject({ phase: 'finished', ending: 'ko' });
  });

  it('shows the time banner from time_up to the end, with the ko tween', () => {
    const p = byTime('a');
    const codes = (t: number) => at(p, t).banners.map((b) => b.code);
    expect(codes(4999)).toEqual([]);
    expect(codes(5000)).toEqual(['time']);
    expect(codes(9000)).toEqual(['time']);
    expect(at(p, 5000).banners[0]).toMatchObject({ alpha: 0, scale: 1.4 });
    expect(at(p, 5100).banners[0]).toMatchObject({ alpha: 0.5 });
    expect(at(p, 5200).banners[0]).toMatchObject({ alpha: 1, scale: 1 });
    expect(at(p, 5000, true).banners[0]).toMatchObject({ alpha: 1, scale: 1 });
  });

  it('never shows the time banner in a KO', () => {
    const p = prepareScene(timeline_({ version: 2 }).finish(3000, 'b'), MANIFESTS);
    for (const t of times(0, p.durationMs, 50)) {
      expect(at(p, t).banners.map((b) => b.code)).not.toContain('time');
    }
  });
});

describe('endings by time on the engine goldens (005 RF7)', () => {
  it.each([
    ['decision-a', 'win', 'a', 'decision'],
    ['decision-b', 'win', 'b', 'decision'],
    ['draw', 'draw', null, 'draw'],
  ] as const)('%s: phase, winner and ending around time_up and the closing', (name, ...rest) => {
    const [closingPhase, winner, ending] = rest;
    const p = goldenScene(name);
    const s = (t: number) => sceneAt(p, t, OPTS);
    expect(s(85_999)).toMatchObject({ phase: 'fight', winner: null, ending: null });
    expect(s(86_000)).toMatchObject({ phase: 'time_up', winner: null, ending: null });
    expect(s(86_000).banners.map((b) => b.code)).toEqual(['time']);
    expect(s(87_499)).toMatchObject({ phase: 'time_up', ending: null });
    expect(s(87_500)).toMatchObject({ phase: closingPhase, winner, ending });
    expect(s(90_000)).toMatchObject({ phase: 'finished', winner, ending });
    expect(s(90_000).banners.map((b) => b.code)).toEqual(['time']);
  });
});
