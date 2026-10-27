import { describe, expect, it } from 'vitest';
import {
  CONTACT_PX,
  MARK_X,
  RETURN_MS,
  prepareScene,
  sceneAt,
  type FighterScene,
} from '../src/scene/index.ts';
import type { Side, Timeline } from '../src/timeline/index.ts';
import { LONG } from './support/scenes.ts';
import { timeline, type TimelineBuilder } from './support/timeline-builder.ts';

// RF-A9 and spec "Diseño → Coreografía": one test per precedence rule, with minimal timelines
// that force each overlap. Placeholder timings: dummy_a attack_light windup 120 ms (recovery
// 210), attack_heavy windup 300 (recovery 370); dummy_b attack_heavy windup 380. The LONG
// manifests stretch reactions and recoveries to ~2.4 s.
const OPTS = { reducedMotion: false };

function at(tl: Timeline, builder: TimelineBuilder) {
  const prepared = prepareScene(tl, builder.manifests);
  return (t: number, side: Side): FighterScene => sceneAt(prepared, t, OPTS).fighters[side];
}

const CONTACT_A = MARK_X.b - CONTACT_PX;
const CONTACT_B = MARK_X.a + CONTACT_PX;

describe('precedence rules', () => {
  it('starts the windup windup_ms before at_ms, with the impact frame on screen at at_ms', () => {
    const b = timeline();
    const f = at(b.strike(3000, 'a', 'attack_heavy', 'hit', 10).finish(6000), b);
    expect(f(2699, 'a')).toMatchObject({ animation: 'idle', x: MARK_X.a });
    expect(f(2700, 'a')).toMatchObject({ animation: 'attack_heavy', step: 0, x: MARK_X.a });
    expect(f(2850, 'a').x).toBeCloseTo((MARK_X.a + CONTACT_A) / 2);
    expect(f(3000, 'a')).toMatchObject({ animation: 'attack_heavy', step: 3, x: CONTACT_A });
    expect(f(3000, 'b')).toMatchObject({ animation: 'hit', step: 0, x: MARK_X.b });
  });

  it('returns to the mark within RETURN_MS after the recovery', () => {
    const b = timeline();
    const f = at(b.strike(3000, 'a', 'attack_light', 'hit', 10).finish(6000), b);
    // Recovery: 210 ms after the impact, at contact distance.
    expect(f(3209, 'a')).toMatchObject({ animation: 'attack_light', x: CONTACT_A });
    expect(f(3210 + RETURN_MS / 2, 'a').x).toBeCloseTo((CONTACT_A + MARK_X.a) / 2);
    expect(f(3210 + RETURN_MS, 'a')).toMatchObject({ animation: 'idle', x: MARK_X.a });
    expect(RETURN_MS).toBeLessThanOrEqual(300);
  });

  it('combo of the same actor at 600 ms stays at contact distance', () => {
    const b = timeline();
    const f = at(
      b
        .strike(3000, 'a', 'attack_light', 'hit', 10)
        .strike(3600, 'a', 'attack_light', 'hit', 10)
        .finish(6000),
      b,
    );
    for (const t of [3000, 3209, 3300, 3479, 3480, 3550, 3600]) {
      expect(f(t, 'a').x).toBe(CONTACT_A);
    }
    expect(f(3300, 'a').animation).toBe('idle');
    expect(f(3480, 'a')).toMatchObject({ animation: 'attack_light', step: 0 });
    expect(f(3600, 'a')).toMatchObject({ animation: 'attack_light', step: 2 });
  });

  it('alternating strikes at 600 ms: the windup cuts the reaction, the impact cuts the recovery', () => {
    const b = timeline(LONG);
    const f = at(
      b
        .strike(3000, 'a', 'attack_heavy', 'hit', 10)
        .strike(3600, 'b', 'attack_light', 'block', 5)
        .strike(4200, 'a', 'attack_heavy', 'dodge')
        .strike(4800, 'b', 'attack_heavy', 'hit', 10)
        .finish(8000),
      b,
    );
    // b reacts to the first hit (2.4 s long) until its own windup at 3480 cuts it (rule 2).
    expect(f(3400, 'b').animation).toBe('hit');
    expect(f(3480, 'b')).toMatchObject({ animation: 'attack_light', step: 0 });
    // a is at contact distance already, so b does not need to step in.
    expect(f(3600, 'b')).toMatchObject({ animation: 'attack_light', step: 2, x: MARK_X.b });
    // a's long recovery is cut by b's impact (rule 3), and a blocks where it stands.
    expect(f(3599, 'a').animation).toBe('attack_heavy');
    expect(f(3600, 'a')).toMatchObject({ animation: 'block', x: CONTACT_A });
    // a's next windup (300 ms) cuts the block (rule 2).
    expect(f(3899, 'a').animation).toBe('block');
    expect(f(3900, 'a')).toMatchObject({ animation: 'attack_heavy', step: 0 });
    // b dodges, cutting its own recovery; then its windup cuts the dodge.
    expect(f(4199, 'b').animation).toBe('attack_light');
    expect(f(4200, 'b').animation).toBe('dodge');
    expect(f(4499, 'b').animation).toBe('dodge');
    expect(f(4500, 'b')).toMatchObject({ animation: 'attack_heavy', step: 0 });
    expect(f(4800, 'b')).toMatchObject({ animation: 'attack_heavy', step: 3 });
    expect(f(4800, 'a').animation).toBe('hit');
  });

  it('a long reaction plays to its end when nothing cuts it (rule 5)', () => {
    const b = timeline(LONG);
    const f = at(b.strike(3000, 'a', 'attack_light', 'hit', 10).finish(9000), b);
    expect(f(5400, 'b').animation).toBe('hit');
    expect(f(5480, 'b').animation).toBe('idle');
  });

  it('knockdown and ko are never interrupted', () => {
    const b = timeline();
    const f = at(
      b
        .strike(3000, 'a', 'attack_heavy', 'crit', 100)
        .knockdown(3000, 'b', 2000)
        .strike(5600, 'a', 'attack_light', 'hit', 10)
        .finish(7000),
      b,
    );
    for (const t of [3000, 3500, 4000, 4999]) expect(f(t, 'b').animation).toBe('knockdown');
    // dummy_b's knockdown animation lasts 900 ms and holds its last frame.
    expect(f(4999, 'b').step).toBe(3);
    expect(f(5000, 'b').animation).toBe('idle');
    expect(f(5600, 'b').animation).toBe('hit');
    for (const t of [7000, 7500, 8500, 10_000, 20_000]) expect(f(t, 'b').animation).toBe('ko');
    expect(f(20_000, 'b').step).toBe(3);
    expect(f(8500, 'a').animation).toBe('win');
  });

  it('a windup that starts inside an own knockdown waits for its end (decision 15)', () => {
    const b = timeline();
    const f = at(
      b
        .strike(3000, 'a', 'attack_heavy', 'crit', 100)
        .knockdown(3000, 'b', 1500)
        .strike(4600, 'b', 'attack_heavy', 'hit', 10)
        .finish(7000),
      b,
    );
    // b's heavy windup is 380 ms: it would start at 4220, inside the knockdown (until 4500).
    expect(f(4300, 'b').animation).toBe('knockdown');
    expect(f(4499, 'b').animation).toBe('knockdown');
    // From 4500 it plays the frame of t − (4600 − 380): 280 ms in, step 2 of [120,120,140].
    expect(f(4500, 'b')).toMatchObject({ animation: 'attack_heavy', step: 2, x: MARK_X.b });
    expect(f(4550, 'b').x).toBeCloseTo((MARK_X.b + CONTACT_B) / 2);
    expect(f(4600, 'b')).toMatchObject({ animation: 'attack_heavy', step: 3, x: CONTACT_B });
    expect(f(4600, 'a').animation).toBe('hit');
  });
});
