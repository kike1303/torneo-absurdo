import { describe, expect, it } from 'vitest';
import { rootMotion, strideOf, type RootMotion } from '../src/model/gait.ts';
import { BOTARGA_RETURN_MS, paceAssets } from '../src/model/pacing.ts';
import { CLIP_NAMES, OPTIONAL_CLIP_NAMES, poseFor, type ClipDurations } from '../src/model/pose.ts';
import {
  STAGE_METERS_PER_PX,
  STANCE_MARK_X,
  STEP_M,
  metersFromMark,
  stanceScene,
  stanceX,
} from '../src/model/stance.ts';
import { K, cueWindow, prepareScene } from '../src/scene/index.ts';
import { sceneAt } from '../src/scene/scene-at.ts';
import { CONTACT_PX, MARK_X, STAGE_WIDTH, type Scene } from '../src/scene/types.ts';
import { SIDES } from '../src/timeline/types.ts';
import { MANIFESTS } from './support/placeholders.ts';
import { GOLDEN_NAMES, goldenTimeline, spikeTimeline } from './support/scenes.ts';

const DURATIONS = Object.fromEntries(
  [...CLIP_NAMES, ...OPTIONAL_CLIP_NAMES].map((name) => [name, 1]),
) as ClipDurations;
const CONTEXT = { hpFraction: 1, intro: false };
/** The hips of the botarga's walks, as measured on the GLB: 0.18 m forward, 0.20 m back. */
const GAITS = {
  walk_forward: rootMotion([0, 0.25, 0.5, 0.75, 1], [0, 0.03, 0.09, 0.15, 0.18]) as RootMotion,
  walk_back: rootMotion([0, 0.25, 0.5, 0.75, 1], [0, -0.02, -0.09, -0.15, -0.2]) as RootMotion,
};

/** Meters the root of `gait` has advanced at clip time `time` (the inverse of `timeAtDistance`). */
function advanced(gait: RootMotion, time: number): number {
  const { times, distance } = gait;
  let i = 1;
  while (i < times.length - 1 && (times[i] as number) < time) i++;
  const t0 = times[i - 1] as number;
  const t1 = times[i] as number;
  const p = Math.min(1, Math.max(0, (time - t0) / (t1 - t0)));
  return (distance[i - 1] as number) + p * ((distance[i] as number) - (distance[i - 1] as number));
}

/** The fights as the 3D arena plays them: with the botarga's own pace (017) and return. */
const paced = (timeline: Parameters<typeof prepareScene>[0]) =>
  prepareScene(timeline, paceAssets(MANIFESTS) as typeof MANIFESTS, {
    returnMs: BOTARGA_RETURN_MS,
  });
const FIGHTS = [spikeTimeline(), ...GOLDEN_NAMES.map(goldenTimeline)].map(paced);

describe('the 3D stance (TA-74)', () => {
  it('stands the fighters a contact and their steps apart, around the center', () => {
    const gap = (STANCE_MARK_X.b - STANCE_MARK_X.a) * STAGE_METERS_PER_PX;
    expect(gap).toBeCloseTo(CONTACT_PX * STAGE_METERS_PER_PX + STEP_M);
    expect((STANCE_MARK_X.a + STANCE_MARK_X.b) / 2).toBeCloseTo(STAGE_WIDTH / 2);
    for (const side of SIDES) expect(stanceX(side, MARK_X[side])).toBeCloseTo(STANCE_MARK_X[side]);
  });

  it('keeps the contact distance of a strike and makes the way there `STEP_M`', () => {
    const contactA = MARK_X.b - CONTACT_PX;
    expect(stanceX('b', MARK_X.b) - stanceX('a', contactA)).toBeCloseTo(CONTACT_PX);
    expect(metersFromMark({ side: 'a', x: stanceX('a', contactA) })).toBeCloseTo(STEP_M);
    const contactB = MARK_X.a + CONTACT_PX;
    expect(stanceX('b', contactB) - stanceX('a', MARK_X.a)).toBeCloseTo(CONTACT_PX);
    expect(metersFromMark({ side: 'b', x: stanceX('b', contactB) })).toBeCloseTo(STEP_M);
  });

  it('makes the way back a whole number of steps of the clip, at a pace that reads', () => {
    const steps = STEP_M / strideOf(GAITS.walk_back);
    expect(Math.abs(steps - 3)).toBeLessThan(0.1);
    // Steps a second on a whole return: the 2D return (300 ms) would take 10.
    expect(steps / (BOTARGA_RETURN_MS / 1000)).toBeLessThan(3.5);
  });

  it('never lets the fighters cross or stand closer than they do in 2D', () => {
    for (const prepared of FIGHTS) {
      for (let t = 0; t < prepared.durationMs; t += 16) {
        const scene = sceneAt(prepared, t, { reducedMotion: false });
        const staged = stanceScene(scene);
        const apart2d = scene.fighters.b.x - scene.fighters.a.x;
        const apart3d = staged.fighters.b.x - staged.fighters.a.x;
        expect(apart3d, `t=${t}`).toBeGreaterThan(0);
        if (apart2d >= CONTACT_PX)
          expect(apart3d, `t=${t}`).toBeGreaterThanOrEqual(CONTACT_PX - 1e-9);
      }
    }
  });

  it('keeps the effects where they were relative to the fighters', () => {
    const base = sceneAt(FIGHTS[0] as (typeof FIGHTS)[number], 0, { reducedMotion: false });
    const a = { ...base.fighters.a, x: 300 };
    const b = { ...base.fighters.b, x: 400 };
    const particle = { kind: 'particle', y: 0, size: 1, alpha: 1, color: 'text' } as const;
    const scene: Scene = {
      ...base,
      fighters: { a, b },
      camera: { ...base.camera, zoom: 1.2, centerX: 350 },
      effects: [
        { ...particle, x: 300 },
        { ...particle, x: 350 },
        { ...particle, x: 400 },
        { ...particle, x: 420 },
        { ...particle, x: 290 },
        { kind: 'flash', cue: 0, side: 'a', alpha: 1 },
      ],
    };
    const staged = stanceScene(scene);
    const ax = stanceX('a', 300);
    const bx = stanceX('b', 400);
    expect(staged.fighters.a.x).toBeCloseTo(ax);
    expect(staged.fighters.b.x).toBeCloseTo(bx);
    const xs = staged.effects.map((e) => (e.kind === 'flash' ? null : e.x));
    expect(xs[0]).toBeCloseTo(ax);
    expect(xs[1]).toBeCloseTo((ax + bx) / 2);
    expect(xs[2]).toBeCloseTo(bx);
    expect(xs[3]).toBeCloseTo(bx + 20);
    expect(xs[4]).toBeCloseTo(ax - 10);
    expect(staged.effects[5]).toBe(scene.effects[5]);
    expect(staged.camera.centerX).toBeCloseTo((ax + bx) / 2);
    // Without a zoom the camera looks at the middle of the stage, wherever the fighters are.
    const still = stanceScene({ ...scene, camera: base.camera });
    expect(still.camera).toBe(base.camera);
  });
});

describe("the botarga's return (TA-74)", () => {
  it('takes its own time in 3D and leaves the default return alone', () => {
    const timeline = spikeTimeline();
    const slow = FIGHTS[0] as (typeof FIGHTS)[number];
    const fast = prepareScene(timeline, paceAssets(MANIFESTS) as typeof MANIFESTS);
    expect(slow.returnMs).toBe(BOTARGA_RETURN_MS);
    expect(fast.returnMs).toBe(300);
    const longest = (prepared: typeof slow) =>
      Math.max(
        ...prepared.cues.flatMap((pc) =>
          pc.segments.filter((s) => s.kind === 'return').map((s) => s.end - s.start),
        ),
      );
    expect(longest(slow)).toBe(BOTARGA_RETURN_MS);
    expect(longest(fast)).toBe(300);
  });

  it('does not move a strike: every impact lands on its cue', () => {
    for (const prepared of FIGHTS) {
      for (const pc of prepared.cues) {
        if (pc.cue.type !== 'strike') continue;
        const striker = sceneAt(prepared, pc.cue.at_ms, { reducedMotion: false }).fighters[
          pc.cue.actor
        ];
        if (striker.animation === pc.cue.move) expect(striker.elapsed).toBe(striker.windup);
      }
    }
  });

  it('keeps the cue window bounded with the longer return', () => {
    for (const prepared of FIGHTS) {
      for (let t = 0; t < prepared.durationMs; t += 50) {
        expect(cueWindow(prepared, t).length).toBeLessThanOrEqual(K);
      }
    }
  });
});

describe('a walk follows the ground (TA-74)', () => {
  it('shows the frame whose root has advanced what the fighter has walked', () => {
    let walks = 0;
    for (const prepared of FIGHTS) {
      for (let t = 0; t < prepared.durationMs; t += 5) {
        const staged = stanceScene(sceneAt(prepared, t, { reducedMotion: false }));
        for (const side of SIDES) {
          const f = staged.fighters[side];
          if (f.animation !== 'walk') continue;
          walks++;
          const pose = poseFor(f, t, DURATIONS, CONTEXT, undefined, GAITS);
          expect(pose.clip).toBe('walk_back');
          const gait = GAITS.walk_back;
          // Steps are counted back from the mark: the root of the clip is as far from the end of
          // its step as the fighter is from the end of that step on the floor, so a planted foot
          // stays where it is.
          const stride = strideOf(gait);
          const left = stride - advanced(gait, pose.time);
          const toStepEnd = metersFromMark(f) % stride;
          const slip = Math.min(Math.abs(left - toStepEnd), stride - Math.abs(left - toStepEnd));
          expect(slip, `${side} t=${t}`).toBeLessThan(1e-6);
        }
      }
    }
    expect(walks).toBeGreaterThan(50);
  });

  it('is a function of the position alone: no clock, the same after a seek', () => {
    const f = { side: 'a', x: stanceX('a', MARK_X.a + 60), travel: -1 } as const;
    const scene = sceneAt(FIGHTS[0] as (typeof FIGHTS)[number], 0, { reducedMotion: false });
    const walking = { ...scene.fighters.a, ...f, animation: 'walk' as const };
    const at = (t: number, elapsed: number) =>
      poseFor({ ...walking, elapsed }, t, DURATIONS, CONTEXT, undefined, GAITS).time;
    expect(at(1000, 10)).toBe(at(90_000, 250));
  });

  it('walks towards the rival with the forward clip, a stride per play', () => {
    const scene = sceneAt(FIGHTS[0] as (typeof FIGHTS)[number], 0, { reducedMotion: false });
    const at = (meters: number) =>
      poseFor(
        {
          ...scene.fighters.a,
          animation: 'walk',
          travel: 1,
          x: STANCE_MARK_X.a + meters / STAGE_METERS_PER_PX,
        },
        0,
        DURATIONS,
        CONTEXT,
        undefined,
        GAITS,
      );
    expect(at(0.09).clip).toBe('walk_forward');
    expect(advanced(GAITS.walk_forward, at(0.09).time)).toBeCloseTo(0.09);
    expect(at(0.09 + 0.18).time).toBeCloseTo(at(0.09).time);
  });

  it('keeps the timed step for a walk clip without root motion', () => {
    const scene = sceneAt(FIGHTS[0] as (typeof FIGHTS)[number], 0, { reducedMotion: false });
    const walking = { ...scene.fighters.a, animation: 'walk' as const, travel: -1 as const };
    const pose = poseFor({ ...walking, elapsed: 150 }, 0, DURATIONS, CONTEXT, undefined, {});
    expect(pose.time).toBeCloseTo(0.25);
  });
});
