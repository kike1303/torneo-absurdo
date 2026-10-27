import dummyA from '../placeholders/dummy_a/manifest.json' with { type: 'json' };
import dummyB from '../placeholders/dummy_b/manifest.json' with { type: 'json' };
import spike from '../fixtures/spike.timeline.json' with { type: 'json' };
import { describe, expect, it } from 'vitest';
import { RendererError, createPlayer, type PlayerState } from '../src/player/index.ts';
import type { Cue, StrikeCue } from '../src/timeline/index.ts';
import { FakeClock, FakeStage, options } from './support/fake-stage.ts';

// RF-A11, RF-A14 (test 7 of ADR-0009) and RF-A19, plan-a2 decisions 2 to 4.
const ASSETS = new Map<string, unknown>([
  ['dummy_a', dummyA],
  ['dummy_b', dummyB],
]);
const CUES = spike.cues as unknown as Cue[];
const DURATION = spike.duration_ms;
const STRIKES = CUES.filter((c): c is StrikeCue => c.type === 'strike');

async function setup(reducedMotion = false) {
  const clock = new FakeClock();
  const stage = new FakeStage();
  const player = createPlayer(options(clock, { reducedMotion }), stage);
  await player.load(spike, ASSETS);
  const fired: [number, number][] = [];
  player.onCue((_cue, index) => fired.push([index, clock.now]));
  const states: PlayerState[] = [];
  player.onState((s) => states.push(s));
  /** Advances the clock to `to` in display frames of 16 ms. */
  const runTo = (to: number, step = 16) => {
    while (clock.now < to) {
      const dt = Math.min(step, to - clock.now);
      clock.now += dt;
      stage.frame(dt);
    }
  };
  return { clock, stage, player, fired, states, runTo };
}

describe('the player (RF-A11)', () => {
  it('is ready after load, with a scene at t = 0', async () => {
    const { player, stage } = await setup();
    expect(stage.loaded?.size).toBe(2);
    expect(player.snapshot()).toMatchObject({ t: 0, phase: 'intro', hp: { a: 1000, b: 1000 } });
    expect(stage.last?.t).toBe(0);
  });

  it('fires onCue once per cue when its at_ms passes, in order', async () => {
    const { player, fired, runTo } = await setup();
    player.seek(0);
    player.play();
    runTo(DURATION + 100);
    expect(fired.map(([i]) => i)).toEqual(CUES.map((_, i) => i));
    for (const [index, at] of fired) {
      const cue = CUES[index] as Cue;
      expect(at).toBeGreaterThanOrEqual(cue.at_ms);
      expect(at).toBeLessThanOrEqual(cue.at_ms + 16);
    }
  });

  it('a seek does not fire the cues it skips', async () => {
    const { player, fired, clock, runTo } = await setup();
    player.seek(0);
    player.play();
    runTo(1000);
    const target = STRIKES[10]?.at_ms ?? 0;
    player.seek(target - 50);
    runTo(clock.now + 100);
    expect(fired.map(([i]) => i)).toEqual([0, CUES.findIndex((c) => c.at_ms === target)]);
  });

  it('pause freezes t and play goes on without a jump', async () => {
    const { player, clock, runTo, states } = await setup();
    player.seek(0);
    player.play();
    runTo(5000);
    player.pause();
    const t = player.snapshot()?.t ?? -1;
    runTo(9000);
    expect(player.snapshot()?.t).toBe(t);
    player.play();
    runTo(clock.now + 1000);
    expect(player.snapshot()?.t).toBe(t + 1000);
    expect(states).toEqual(['playing', 'paused', 'playing']);
  });

  it('a 10 s jump of the clock fires every cue of the interval in order', async () => {
    const { player, fired, clock, stage, runTo } = await setup();
    player.seek(0);
    player.play();
    runTo(2000);
    clock.now += 10_000;
    stage.frame(10_000);
    const expected = CUES.map((c, i) => [c, i] as const)
      .filter(([c]) => c.at_ms <= 12_000)
      .map(([, i]) => i);
    expect(fired.map(([i]) => i)).toEqual(expected);
    expect(fired.filter(([, at]) => at === 12_000).length).toBeGreaterThan(1);
  });

  it('onState reaches finished at duration_ms', async () => {
    const { player, runTo, states } = await setup();
    player.seek(0);
    player.play();
    runTo(DURATION - 16);
    expect(states.at(-1)).toBe('playing');
    runTo(DURATION + 32);
    expect(states.at(-1)).toBe('finished');
    expect(player.snapshot()).toMatchObject({ t: DURATION, phase: 'finished', winner: 'a' });
  });

  it('seeking to the end finishes, and back to 0 plays again', async () => {
    const { player, runTo, states, clock } = await setup();
    player.seek(DURATION);
    player.play();
    expect(states.at(-1)).toBe('finished');
    player.seek(0);
    runTo(clock.now + 100);
    expect(states.at(-1)).toBe('playing');
  });

  it('snapshot().hp is the target_hp of the last strike', async () => {
    const { player, runTo } = await setup();
    player.seek(0);
    player.play();
    const strike = STRIKES[7] as StrikeCue;
    runTo(strike.at_ms + 20);
    const target = strike.actor === 'a' ? 'b' : 'a';
    const snap = player.snapshot();
    expect(snap?.hp[target]).toBe(strike.target_hp);
    expect(snap?.lastCue?.type).toBe('strike');
  });

  it('snapshot() inside a cue listener already shows that cue', async () => {
    const { player, runTo } = await setup();
    const seen: [number, number | undefined][] = [];
    player.onCue((_cue, index) => seen.push([index, player.snapshot()?.lastCue?.index]));
    player.seek(0);
    player.play();
    runTo(20_000);
    expect(seen.length).toBeGreaterThan(5);
    // Several cues can share an at_ms (a strike and its knockdown): the last one of the frame.
    for (const [index, last] of seen) expect(last).toBeGreaterThanOrEqual(index);
  });

  it('live: without seek nor pause, t = clock()', async () => {
    const clock = new FakeClock();
    const stage = new FakeStage();
    const player = createPlayer(options(clock), stage);
    await player.load(spike, ASSETS);
    clock.now = 30_000;
    player.play();
    stage.frame();
    expect(player.snapshot()?.t).toBe(30_000);
  });

  it('frame stats only count while playing, between the first strike and the win', async () => {
    const { player, runTo, clock, stage } = await setup();
    player.seek(0);
    player.play();
    runTo((STRIKES[0]?.at_ms ?? 0) - 100);
    expect(player.getFrameStats().frames).toBe(0);
    runTo(clock.now + 1600);
    const frames = player.getFrameStats().frames;
    // 1 600 ms of 16 ms frames, minus the ~100 ms before the first strike.
    expect(frames).toBeGreaterThanOrEqual(93);
    expect(frames).toBeLessThanOrEqual(95);
    player.pause();
    stage.frame(16);
    clock.now += 16;
    expect(player.getFrameStats().frames).toBe(frames);
    player.seek(30_000);
    expect(player.getFrameStats().frames).toBe(0);
  });

  it('rejects with codes, before touching the stage', async () => {
    const clock = new FakeClock();
    const stage = new FakeStage();
    const player = createPlayer(options(clock), stage);
    await expect(player.load({ ...spike, timeline_version: 9 }, ASSETS)).rejects.toMatchObject({
      code: 'timeline_invalid',
    });
    const bad = new Map<string, unknown>([
      ...ASSETS,
      ['dummy_b', { ...dummyB, format_version: 2 }],
    ]);
    await expect(player.load(spike, bad)).rejects.toBeInstanceOf(RendererError);
    await expect(player.load(spike, bad)).rejects.toMatchObject({ code: 'asset_invalid' });
    expect(stage.loaded).toBeNull();
  });

  it('propagates webgl_unavailable and asset_load_failed', async () => {
    const clock = new FakeClock();
    const noGl = new FakeStage();
    noGl.failInit = true;
    await expect(createPlayer(options(clock), noGl).load(spike, ASSETS)).rejects.toMatchObject({
      code: 'webgl_unavailable',
    });
    const noAssets = new FakeStage();
    noAssets.failLoad = true;
    await expect(createPlayer(options(clock), noAssets).load(spike, ASSETS)).rejects.toMatchObject({
      code: 'asset_load_failed',
    });
  });

  it('destroy stops everything', async () => {
    const { player, stage, fired, runTo } = await setup();
    player.seek(0);
    player.play();
    player.destroy();
    runTo(5000);
    expect(stage.destroyed).toBe(true);
    expect(fired).toEqual([]);
  });
});

describe('reduced motion never changes the timing (ADR-0009 test 7, RF-A14)', () => {
  const run = async (flag: (t: number) => boolean) => {
    const s = await setup(flag(0));
    s.player.seek(0);
    s.player.play();
    let last = flag(0);
    while (s.clock.now < DURATION + 100) {
      s.clock.now += 16;
      const next = flag(s.clock.now);
      if (next !== last) s.player.setReducedMotion(next);
      last = next;
      s.stage.frame(16);
    }
    return s;
  };

  it('the (cue index, firing time) sequence is identical with true and false', async () => {
    const off = await run(() => false);
    const on = await run(() => true);
    expect(on.fired).toEqual(off.fired);
    expect(on.fired).toHaveLength(CUES.length);
  });

  it('also when the flag changes in the middle of the playback', async () => {
    const off = await run(() => false);
    const switched = await run((t) => t > 20_000 && t < 45_000);
    expect(switched.fired).toEqual(off.fired);
    // The fighters and the HUD are the same; only the camera and the effects may differ.
    const pick = (s: (typeof off)['stage']['scenes'][number]) => ({
      f: s.fighters,
      h: s.hud,
      t: s.t,
    });
    expect(switched.stage.scenes.map(pick)).toEqual(off.stage.scenes.map(pick));
  });
});
