import dummyA from '../placeholders/dummy_a/manifest.json' with { type: 'json' };
import dummyB from '../placeholders/dummy_b/manifest.json' with { type: 'json' };
import spike from '../fixtures/spike.timeline.json' with { type: 'json' };
import { describe, expect, it } from 'vitest';
import type { SpriteManifest } from '../src/manifest/index.ts';
import { RendererError, createPlayer, type PlayerState } from '../src/player/index.ts';
import type { Cue, Timeline } from '../src/timeline/index.ts';
import { FakeClock, FakeStage, options } from './support/fake-stage.ts';
import { goldenTimeline } from './support/scenes.ts';

// 010 RF47, RF48 and RF50: a fight in progress, with a fake `Stage`.
const ASSETS = new Map<string, unknown>([
  ['dummy_a', dummyA],
  ['dummy_b', dummyB],
]);
const PROVISIONAL_MS = 90_000;
const DRAW = goldenTimeline('draw');
const CUES: readonly Cue[] = DRAW.cues;
const CLOSING_AT = (CUES.at(-1) as Cue).at_ms;

/** What the adapter gives after the events up to `c`: no closing and a provisional duration. */
function cut(c: number, timeline: Timeline = DRAW): Timeline {
  return {
    ...timeline,
    duration_ms: PROVISIONAL_MS,
    cues: timeline.cues.filter((q) => q.at_ms <= c && q.type !== 'win' && q.type !== 'draw'),
  };
}

class CountingStage extends FakeStage {
  loads = 0;
  override async loadAssets(manifests: ReadonlyMap<string, SpriteManifest>): Promise<void> {
    this.loads++;
    await super.loadAssets(manifests);
  }
}

/** A stage whose sheets stay loading until `release`. */
class SlowStage extends FakeStage {
  release: () => void = () => undefined;
  override async loadAssets(manifests: ReadonlyMap<string, SpriteManifest>): Promise<void> {
    await new Promise<void>((resolve) => {
      this.release = resolve;
    });
    await super.loadAssets(manifests);
  }
}

async function setup(first: Timeline = cut(10_000), now = 0) {
  const clock = new FakeClock();
  clock.now = now;
  const stage = new CountingStage();
  const player = createPlayer(options(clock), stage);
  await player.loadLive(first, ASSETS);
  const fired: number[] = [];
  player.onCue((_cue, index) => fired.push(index));
  const states: PlayerState[] = [];
  player.onState((s) => states.push(s));
  return { clock, stage, player, fired, states };
}

const rejected = (fn: () => void) => {
  try {
    fn();
  } catch (error) {
    return error instanceof RendererError ? error.code : error;
  }
  return null;
};

describe('loadLive (010 RF47)', () => {
  it('takes a partial timeline, is ready and shows the scene at t = 0', async () => {
    const { player, stage } = await setup();
    expect(stage.loads).toBe(1);
    expect(player.snapshot()).toMatchObject({ t: 0, phase: 'intro' });
  });

  it('load does not take a partial timeline: load has not changed', async () => {
    const player = createPlayer(options(new FakeClock()), new FakeStage());
    await expect(player.load(cut(10_000), ASSETS)).rejects.toMatchObject({
      code: 'timeline_invalid',
    });
  });

  it('rejects an invalid timeline, a v1 one and a bad manifest with their codes', async () => {
    const player = createPlayer(options(new FakeClock()), new FakeStage());
    await expect(player.loadLive(spike, ASSETS)).rejects.toMatchObject({
      code: 'timeline_invalid',
    });
    await expect(player.loadLive({ ...cut(10_000), cues: [] }, ASSETS)).rejects.toMatchObject({
      code: 'timeline_invalid',
    });
    await expect(
      player.loadLive(cut(10_000), new Map<string, unknown>([['dummy_a', {}]])),
    ).rejects.toMatchObject({ code: 'asset_invalid' });
  });

  it('an extend during the load is not lost: the newest timeline gets installed', async () => {
    const clock = new FakeClock();
    const stage = new SlowStage();
    const player = createPlayer(options(clock), stage);
    const loading = player.loadLive(cut(10_000), ASSETS);
    player.extend(DRAW);
    await Promise.resolve();
    await Promise.resolve();
    stage.release();
    await loading;
    clock.now = PROVISIONAL_MS;
    player.play();
    stage.frame();
    expect(player.snapshot()).toMatchObject({ t: DRAW.duration_ms, ending: 'draw' });
  });

  it('a failed load leaves no fight to extend', async () => {
    const stage = new FakeStage();
    stage.failLoad = true;
    const player = createPlayer(options(new FakeClock()), stage);
    await expect(player.loadLive(cut(10_000), ASSETS)).rejects.toMatchObject({
      code: 'asset_load_failed',
    });
    expect(rejected(() => player.extend(cut(20_000)))).toBe('timeline_invalid');
  });
});

describe('extend (010 RF47)', () => {
  it('does not reload sheets nor move t or the cursor', async () => {
    const { player, stage, clock, fired } = await setup();
    player.play();
    clock.now = 5000;
    stage.frame();
    const before = [...fired];
    const scenes = stage.scenes.length;
    player.extend(cut(20_000));
    expect(stage.loads).toBe(1);
    expect(stage.scenes.length).toBe(scenes);
    expect(player.snapshot()?.t).toBe(5000);
    stage.frame();
    expect(player.snapshot()?.t).toBe(5000);
    expect(fired).toEqual(before);
  });

  it('the new cues fire in order when the playback reaches them', async () => {
    const { player, stage, clock, fired } = await setup();
    player.play();
    player.extend(cut(30_000));
    clock.now = 30_000;
    stage.frame();
    const expected = CUES.flatMap((c, i) => (c.at_ms <= 30_000 ? [i] : []));
    expect(fired).toEqual(expected);
  });

  it('late cues, already behind t, fire on the next frame without moving t', async () => {
    const { player, stage, clock, fired } = await setup();
    player.play();
    clock.now = 25_000;
    stage.frame();
    const known = fired.length;
    player.extend(cut(20_000));
    stage.frame();
    expect(fired.length).toBeGreaterThan(known);
    expect(player.snapshot()?.t).toBe(25_000);
  });

  it('accepts the same cues again (a fight_sync after a reconnection)', async () => {
    const { player, stage, clock, fired } = await setup();
    player.play();
    clock.now = 5000;
    stage.frame();
    const before = [...fired];
    expect(rejected(() => player.extend(cut(10_000)))).toBeNull();
    stage.frame();
    expect(fired).toEqual(before);
  });

  it('updates the duration when the closing arrives', async () => {
    const { player, stage, clock, states } = await setup(cut(10_000));
    player.play();
    clock.now = DRAW.duration_ms - 1;
    stage.frame();
    expect(states).not.toContain('finished');
    player.extend(DRAW);
    clock.now = DRAW.duration_ms;
    stage.frame();
    expect(states.at(-1)).toBe('finished');
  });

  describe('rejects with timeline_invalid and changes nothing', () => {
    const base = cut(20_000);
    const strikeAt = base.cues.findIndex((c) => c.type === 'strike');
    const changed = (edit: (cues: Cue[]) => void): Timeline => {
      const cues = [...base.cues];
      edit(cues);
      return { ...base, cues };
    };
    const CASES: readonly (readonly [string, unknown])[] = [
      ['not valid', { ...base, cues: [] }],
      ['v1', spike],
      ['a shorter list', cut(10_000)],
      ['an earlier cue removed', changed((cues) => cues.splice(strikeAt, 1))],
      [
        'an earlier cue changed',
        changed((cues) => {
          const strike = cues[strikeAt] as Cue & { type: 'strike' };
          cues[strikeAt] = {
            ...strike,
            move: strike.move === 'special' ? 'attack_light' : 'special',
          };
        }),
      ],
      [
        'another asset',
        { ...base, fighters: { ...base.fighters, a: { ...base.fighters.a, asset: 'dummy_b' } } },
      ],
      [
        'another max_hp',
        {
          ...base,
          fighters: {
            ...base.fighters,
            b: { ...base.fighters.b, max_hp: base.fighters.b.max_hp + 1 },
          },
        },
      ],
      ['another duration without the closing', { ...base, duration_ms: PROVISIONAL_MS - 1 }],
    ];

    it.each(CASES)('%s', async (_name, json) => {
      const { player, stage, clock } = await setup(base);
      player.play();
      clock.now = 15_000;
      stage.frame();
      const before = player.snapshot();
      expect(rejected(() => player.extend(json))).toBe('timeline_invalid');
      stage.frame();
      expect(player.snapshot()).toEqual(before);
      // Still extendable: the rejected timeline did not replace the good one.
      expect(rejected(() => player.extend(cut(30_000)))).toBeNull();
    });

    it('another duration after the closing', async () => {
      const { player } = await setup(DRAW);
      expect(rejected(() => player.extend({ ...DRAW, duration_ms: 91_000 }))).toBe(
        'timeline_invalid',
      );
    });

    it('without a loadLive, after a load and after a failed one', async () => {
      const clock = new FakeClock();
      const stage = new FakeStage();
      const player = createPlayer(options(clock), stage);
      expect(rejected(() => player.extend(base))).toBe('timeline_invalid');
      await player.loadLive(base, ASSETS);
      expect(rejected(() => player.extend(cut(30_000)))).toBeNull();
      await player.load(DRAW, ASSETS);
      expect(rejected(() => player.extend(cut(30_000)))).toBe('timeline_invalid');
    });
  });

  it('after destroy it does nothing', async () => {
    const { player } = await setup();
    player.destroy();
    expect(rejected(() => player.extend('not a timeline'))).toBeNull();
  });

  it('an extend inside a cue listener does not lose the rest of the batch', async () => {
    const { player, stage, clock, fired } = await setup(cut(30_000));
    let extended = false;
    player.onCue(() => {
      if (extended) return;
      extended = true;
      player.extend(cut(40_000));
    });
    player.play();
    clock.now = 30_000;
    stage.frame();
    expect(extended).toBe(true);
    expect(fired).toEqual(CUES.flatMap((c, i) => (c.at_ms <= 30_000 ? [i] : [])));
    clock.now = 40_000;
    stage.frame();
    expect(fired).toEqual(CUES.flatMap((c, i) => (c.at_ms <= 40_000 ? [i] : [])));
  });

  it('a load inside a cue listener only takes over once installed', async () => {
    const { player, stage, clock, fired } = await setup(cut(30_000));
    let loading: Promise<void> | null = null;
    player.onCue(() => {
      loading ??= player.load(DRAW, ASSETS);
    });
    player.play();
    clock.now = 30_000;
    stage.frame();
    // The load is not installed yet, so this batch goes on, as before this feature.
    expect(fired.length).toBeGreaterThan(1);
    await loading;
    expect(player.snapshot()?.t).toBe(0);
  });
});

describe('the provisional duration (010 RF48)', () => {
  it('without the closing there is no finished, in the state nor in the phase', async () => {
    const { player, stage, clock, states } = await setup();
    player.play();
    clock.now = PROVISIONAL_MS + 60_000;
    stage.frame();
    expect(states).not.toContain('finished');
    expect(player.snapshot()?.t).toBe(PROVISIONAL_MS);
    expect(player.snapshot()?.phase).not.toBe('finished');
    expect(stage.last?.phase).not.toBe('finished');
    player.pause();
    expect(states.at(-1)).toBe('paused');
    player.seek(PROVISIONAL_MS);
    expect(states.at(-1)).toBe('paused');
    player.play();
    expect(states.at(-1)).toBe('playing');
  });

  it('with the closing it finishes at end.t, and a late end finishes at once', async () => {
    const { player, stage, clock, states } = await setup();
    player.play();
    clock.now = PROVISIONAL_MS + 60_000;
    stage.frame();
    player.extend(DRAW);
    stage.frame();
    expect(states.at(-1)).toBe('finished');
    expect(player.snapshot()).toMatchObject({ t: DRAW.duration_ms, phase: 'finished' });
  });

  it('the snapshot has no duration nor progress', async () => {
    const { player } = await setup();
    const keys = Object.keys(player.snapshot() ?? {}).sort();
    expect(keys).toEqual(['ending', 'hp', 'lastCue', 'phase', 't', 'winner']);
  });
});

describe('coming in late (010 RF50)', () => {
  it('play() on an advanced clock fires nothing of what it skipped', async () => {
    const { player, stage, clock, fired, states } = await setup(cut(40_000), 30_000);
    player.play();
    expect(states.at(-1)).toBe('playing');
    stage.frame();
    expect(fired).toEqual([]);
    expect(player.snapshot()?.t).toBe(30_000);
    clock.now = 40_000;
    stage.frame();
    expect(fired).toEqual(
      CUES.flatMap((c, i) => (c.at_ms > 30_000 && c.at_ms <= 40_000 ? [i] : [])),
    );
  });

  it('a cue exactly at t still fires, on the next frame', async () => {
    const index = CUES.findIndex((c) => c.at_ms > 20_000);
    const at = (CUES[index] as Cue).at_ms;
    const { player, stage, fired } = await setup(cut(40_000), at);
    player.play();
    stage.frame();
    expect(fired).toEqual([index]);
  });

  it('after the closing it finishes at once, without firing it', async () => {
    const { player, stage, fired, states } = await setup(DRAW, DRAW.duration_ms + 1000);
    player.play();
    expect(states.at(-1)).toBe('finished');
    stage.frame();
    expect(fired).toEqual([]);
    expect(player.snapshot()).toMatchObject({ ending: 'draw' });
  });

  it('after the end, the first play() already shows the result, without a frame', async () => {
    const { player, stage } = await setup(DRAW, DRAW.duration_ms + 1000);
    const seen: unknown[] = [];
    player.onState((state) => seen.push([state, player.snapshot()?.ending]));
    player.play();
    expect(seen).toEqual([['finished', 'draw']]);
    expect(player.snapshot()).toMatchObject({
      t: DRAW.duration_ms,
      phase: 'finished',
      ending: 'draw',
    });
    expect(stage.last?.t).toBe(DRAW.duration_ms);
  });

  it('in the middle, the first play() already shows that moment, without a frame', async () => {
    const lastStrike = CUES.filter((c) => c.at_ms <= 30_000).findLast((c) => c.type === 'strike');
    const { player } = await setup(cut(40_000), 30_000);
    player.play();
    const snapshot = player.snapshot();
    expect(snapshot?.t).toBe(30_000);
    expect(snapshot?.lastCue?.index).toBe(CUES.lastIndexOf(lastStrike as Cue));
    expect(snapshot?.hp).not.toEqual({ a: DRAW.fighters.a.max_hp, b: DRAW.fighters.b.max_hp });
  });

  it('resuming after a pause does not move the cursor', async () => {
    const { player, stage, clock, fired } = await setup(cut(40_000), 30_000);
    player.play();
    stage.frame();
    player.pause();
    clock.now = 35_000;
    player.play();
    clock.now = 45_000;
    stage.frame();
    // 15 s after the pause point: the cues of (30 000, 40 000] fire, none was lost.
    expect(fired).toEqual(
      CUES.flatMap((c, i) => (c.at_ms > 30_000 && c.at_ms <= 40_000 ? [i] : [])),
    );
  });

  it('with the clock before the start, t = 0 and no cue fires until it reaches 0', async () => {
    const { player, stage, clock, fired } = await setup(cut(10_000), -3500);
    player.play();
    stage.frame();
    expect(player.snapshot()?.t).toBe(0);
    expect(fired).toEqual([]);
    clock.now = -1;
    stage.frame();
    expect(player.snapshot()?.t).toBe(0);
    expect(fired).toEqual([]);
    clock.now = 0;
    stage.frame();
    expect(fired).toEqual([0]);
  });

  it('while a loadLive is loading, the previous fight does not inherit the live rules', async () => {
    const clock = new FakeClock();
    const stage = new SlowStage();
    const player = createPlayer(options(clock), stage);
    const first = player.load(DRAW, ASSETS);
    await Promise.resolve();
    await Promise.resolve();
    stage.release();
    await first;
    const fired: number[] = [];
    player.onCue((_cue, i) => fired.push(i));
    void player.loadLive(cut(10_000), ASSETS);
    clock.now = 30_000;
    player.play();
    stage.frame();
    // RF50 is only for the installed live fight: this `load` fires the interval, as it always did.
    expect(fired).toEqual(CUES.flatMap((c, i) => (c.at_ms <= 30_000 ? [i] : [])));
  });

  it('load keeps firing the whole interval on an advanced clock: load has not changed', async () => {
    const clock = new FakeClock();
    const stage = new FakeStage();
    const player = createPlayer(options(clock), stage);
    await player.load(DRAW, ASSETS);
    const fired: number[] = [];
    player.onCue((_cue, i) => fired.push(i));
    clock.now = 30_000;
    player.play();
    stage.frame();
    expect(fired).toEqual(CUES.flatMap((c, i) => (c.at_ms <= 30_000 ? [i] : [])));
    expect(CLOSING_AT).toBeGreaterThan(30_000);
  });
});
