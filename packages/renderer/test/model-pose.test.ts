import { describe, expect, it } from 'vitest';
import {
  CLIP_IMPACT,
  CLIP_NAMES,
  OPTIONAL_CLIP_NAMES,
  clipFor,
  clipTime,
  faceFor,
  poseFor,
  type ClipDurations,
} from '../src/model/pose.ts';
import { sceneAt } from '../src/scene/scene-at.ts';
import type { FighterScene } from '../src/scene/types.ts';
import { GOLDEN_NAMES, goldenScene, spikeScene } from './support/scenes.ts';

const DURATIONS = Object.fromEntries(CLIP_NAMES.map((name) => [name, 2])) as ClipDurations;
const CONTEXT = { hpFraction: 1, intro: false };

function fighter(overrides: Partial<FighterScene>): FighterScene {
  return {
    side: 'a',
    asset: 'dummy_a',
    animation: 'idle',
    step: 0,
    frame: 0,
    elapsed: 0,
    windup: 0,
    total: 600,
    x: 0,
    travel: 0,
    y: 0,
    mirror: false,
    scale: 1,
    ...overrides,
  };
}

describe('clipTime', () => {
  it('lands the impact of the clip on the impact of the sprite', () => {
    const f = { animation: 'attack_light' as const, windup: 300, total: 700 };
    const impact = (CLIP_IMPACT.attack_light ?? 0) * 2;
    expect(clipTime({ ...f, elapsed: 0 }, 'attack_light', 2, 0)).toBe(0);
    expect(clipTime({ ...f, elapsed: 300 }, 'attack_light', 2, 0)).toBeCloseTo(impact);
    expect(clipTime({ ...f, elapsed: 700 }, 'attack_light', 2, 0)).toBeCloseTo(2);
    expect(clipTime({ ...f, elapsed: 5000 }, 'attack_light', 2, 0)).toBeCloseTo(2);
  });

  it('stretches a reaction to the sprite length', () => {
    const f = { animation: 'hit' as const, windup: 0, total: 400 };
    expect(clipTime({ ...f, elapsed: 200 }, 'hit', 1.5, 0)).toBeCloseTo(0.75);
    expect(clipTime({ ...f, elapsed: 900 }, 'hit', 1.5, 0)).toBeCloseTo(1.5);
  });

  it('loops idle and holds ko and win at their own speed', () => {
    // Idle follows the global phase of t, whatever its own start.
    expect(
      clipTime({ animation: 'idle', elapsed: 40, windup: 0, total: 800 }, 'idle', 2, 2500),
    ).toBe(0.5);
    const ko = { animation: 'ko' as const, windup: 0, total: 800 };
    expect(clipTime({ ...ko, elapsed: 1000 }, 'ko_1', 5, 0)).toBe(1);
    expect(clipTime({ ...ko, elapsed: 60_000 }, 'ko_1', 5, 0)).toBe(5);
  });
});

describe('clipFor', () => {
  it('keeps the variant for the whole animation and depends only on its start', () => {
    const at = (elapsed: number, t: number) =>
      clipFor(fighter({ animation: 'special', elapsed }), t);
    const first = at(0, 1000);
    for (let e = 16; e < 900; e += 16) expect(at(e, 1000 + e)).toBe(first);
    const seen = new Set<string>();
    for (let start = 0; start < 40_000; start += 997) seen.add(at(0, start));
    expect([...seen].sort()).toEqual(['special_1', 'special_2', 'special_3', 'special_4']);
  });

  it('maps every animation code to a clip of the GLB', () => {
    const codes = [
      'idle',
      'intro',
      'walk',
      'attack_light',
      'attack_heavy',
      'special',
      'hit',
      'knockdown',
      'block',
      'dodge',
      'ko',
      'win',
    ] as const;
    for (const animation of codes) {
      expect([...CLIP_NAMES, ...OPTIONAL_CLIP_NAMES]).toContain(clipFor(fighter({ animation }), 0));
    }
  });
});

describe('faceFor', () => {
  it('prepares before the impact and strikes after it', () => {
    const attack = { animation: 'attack_heavy' as const, windup: 300 };
    expect(faceFor({ ...attack, elapsed: 100 }, CONTEXT)).toBe('preparacion');
    expect(faceFor({ ...attack, elapsed: 300 }, CONTEXT)).toBe('golpe');
  });

  it('shows the reaction, the ending and the tired idle', () => {
    const at = (animation: FighterScene['animation'], context = CONTEXT) =>
      faceFor({ animation, elapsed: 0, windup: 0 }, context);
    expect(at('hit')).toBe('dano');
    expect(at('block')).toBe('concentrado');
    expect(at('dodge')).toBe('alerta');
    expect(at('special')).toBe('enfadado');
    expect(at('ko')).toBe('ko');
    expect(at('win')).toBe('victoria');
    expect(at('idle')).toBe('guardia');
    expect(at('idle', { hpFraction: 0.1, intro: false })).toBe('cansado');
    expect(at('idle', { hpFraction: 1, intro: true })).toBe('neutral');
  });
});

describe('poseFor over a real timeline', () => {
  it.each(['spike', ...GOLDEN_NAMES] as const)(
    '%s: one variant per animation, clip time in range',
    (name) => {
      const prepared = name === 'spike' ? spikeScene() : goldenScene(name);
      const last = { a: '', b: '' };
      for (let t = 0; t < prepared.durationMs; t += 16) {
        const scene = sceneAt(prepared, t, { reducedMotion: false });
        for (const side of ['a', 'b'] as const) {
          const f = scene.fighters[side];
          const pose = poseFor(f, t, DURATIONS, CONTEXT);
          expect(pose.time).toBeGreaterThanOrEqual(0);
          expect(pose.time).toBeLessThanOrEqual(2);
          expect(f.elapsed).toBeGreaterThanOrEqual(0);
          // The same animation started at the same instant keeps its clip from frame to frame.
          const key = `${f.animation}@${Math.round(t - f.elapsed)}`;
          if (last[side].startsWith(`${key}=`)) expect(last[side]).toBe(`${key}=${pose.clip}`);
          last[side] = `${key}=${pose.clip}`;
        }
      }
    },
  );
});

describe('walks (014)', () => {
  it('picks the walk clip by the way the fighter moves', () => {
    expect(clipFor(fighter({ animation: 'walk', travel: 1 }), 0)).toBe('walk_forward');
    expect(clipFor(fighter({ animation: 'walk', travel: -1 }), 0)).toBe('walk_back');
  });

  it('steps at the clip pace from the start of the walk', () => {
    const walk = fighter({ animation: 'walk', travel: -1, elapsed: 2500 });
    expect(clipTime(walk, 'walk_back', 2, 99_000)).toBeCloseTo(0.5);
  });

  it('plays idle while the GLB lacks the walk clips', () => {
    const walk = fighter({ animation: 'walk', travel: -1, elapsed: 300 });
    expect(poseFor(walk, 4000, DURATIONS, CONTEXT).clip).toBe('idle');
    const withWalks = { ...DURATIONS, walk_forward: 1, walk_back: 1 };
    expect(poseFor(walk, 4000, withWalks, CONTEXT).clip).toBe('walk_back');
  });

  it('walks back to the mark after an exchange in the spike', () => {
    const prepared = spikeScene();
    const walks: number[] = [];
    for (let t = 0; t < prepared.durationMs; t += 16) {
      const f = sceneAt(prepared, t, { reducedMotion: false }).fighters.a;
      if (f.animation === 'walk') walks.push(f.travel);
    }
    expect(walks.length).toBeGreaterThan(0);
    expect(walks).toContain(-1);
  });
});
