import { describe, expect, it } from 'vitest';
import spike from '../fixtures/spike.timeline.json' with { type: 'json' };
import {
  MOVES,
  validateTimeline,
  type Side,
  type StrikeCue,
  type Timeline,
} from '../src/timeline/index.ts';
import { MANIFESTS } from './support/placeholders.ts';

// RF-A8: the spike script tells a fight with a story and two rhythms.
function load(): Timeline {
  const result = validateTimeline(spike, MANIFESTS);
  if (!result.ok) throw new Error(JSON.stringify(result.errors));
  return result.timeline;
}
const timeline = load();
const strikes = timeline.cues.filter((c): c is StrikeCue => c.type === 'strike');
const winner: Side = timeline.cues.find((c) => c.type === 'win')?.fighter === 'b' ? 'b' : 'a';
const loser: Side = winner === 'a' ? 'b' : 'a';
const target = (s: StrikeCue): Side => (s.actor === 'a' ? 'b' : 'a');

/** HP fraction of each side right after every strike. */
function hpCurve(): { at: number; a: number; b: number }[] {
  const hp = { a: timeline.fighters.a.max_hp, b: timeline.fighters.b.max_hp };
  return strikes.map((s) => {
    hp[target(s)] = s.target_hp;
    return {
      at: s.at_ms,
      a: hp.a / timeline.fighters.a.max_hp,
      b: hp.b / timeline.fighters.b.max_hp,
    };
  });
}

/** HP per second, for the failure messages (fix the JSON without guessing). */
function curveBySecond(): string {
  const curve = hpCurve();
  const lines: string[] = [];
  for (let s = 0; s * 1000 <= timeline.duration_ms; s++) {
    const last = curve.filter((p) => p.at <= s * 1000).at(-1) ?? { a: 1, b: 1 };
    lines.push(`${s}s a=${last.a.toFixed(2)} b=${last.b.toFixed(2)}`);
  }
  return lines.join('\n');
}

describe('spike.timeline.json', () => {
  it('passes validateTimeline with the placeholder manifests and lasts 45 to 90 s', () => {
    expect(timeline.duration_ms).toBeGreaterThanOrEqual(45_000);
    expect(timeline.duration_ms).toBeLessThanOrEqual(90_000);
  });

  it('has at least 30 strikes, every result and a knockdown', () => {
    expect(strikes.length).toBeGreaterThanOrEqual(30);
    const results = new Set(strikes.map((s) => s.result));
    expect([...results].sort()).toEqual(['block', 'crit', 'dodge', 'hit']);
    expect(strikes.some((s) => s.result === 'block' && s.damage > 0)).toBe(true);
    expect(timeline.cues.some((c) => c.type === 'knockdown')).toBe(true);
  });

  it('takes each side below 25 % HP at some point', () => {
    const curve = hpCurve();
    expect(
      curve.some((p) => p.a < 0.25),
      curveBySecond(),
    ).toBe(true);
    expect(
      curve.some((p) => p.b < 0.25),
      curveBySecond(),
    ).toBe(true);
  });

  it('has a burst of ≥ 3 strikes of the same actor at ≤ 900 ms', () => {
    let best = 1;
    let run = 1;
    for (let i = 1; i < strikes.length; i++) {
      const prev = strikes[i - 1];
      const cur = strikes[i];
      const chained =
        prev !== undefined &&
        cur !== undefined &&
        cur.actor === prev.actor &&
        cur.at_ms - prev.at_ms <= 900;
      run = chained ? run + 1 : 1;
      best = Math.max(best, run);
    }
    expect(best).toBeGreaterThanOrEqual(3);
  });

  it('has a pause of ≥ 2.5 s without strikes between the first and the last', () => {
    const gaps = strikes.slice(1).map((s, i) => s.at_ms - (strikes[i]?.at_ms ?? 0));
    expect(Math.max(...gaps)).toBeGreaterThanOrEqual(2500);
  });

  it('has a comeback: the winner trails for ≥ 5 s after 50 % and then retakes the lead', () => {
    const half = timeline.duration_ms / 2;
    const curve = hpCurve();
    let trailingSince: number | null = null;
    let longest = 0;
    let retook = false;
    for (let i = 0; i < curve.length; i++) {
      const p = curve[i];
      if (p === undefined) continue;
      const trailing = p[winner] < p[loser];
      const next = curve[i + 1]?.at ?? timeline.duration_ms;
      if (trailing) {
        trailingSince ??= Math.max(p.at, half);
        if (next > half) longest = Math.max(longest, next - trailingSince);
      } else {
        if (trailingSince !== null && longest >= 5000 && p[winner] > p[loser]) retook = true;
        trailingSince = null;
      }
    }
    expect(longest, curveBySecond()).toBeGreaterThanOrEqual(5000);
    expect(retook, curveBySecond()).toBe(true);
  });

  it('ends with a crit or a special', () => {
    const final = strikes.find((s) => s.target_hp === 0);
    expect(final === undefined ? null : final.result === 'crit' || final.move === 'special').toBe(
      true,
    );
  });

  it.each(['a', 'b'] as const)('side %s uses the three moves and lands a crit', (side) => {
    const own = strikes.filter((s) => s.actor === side);
    expect(new Set(own.map((s) => s.move))).toEqual(new Set(MOVES));
    expect(own.some((s) => s.result === 'crit')).toBe(true);
  });
});
