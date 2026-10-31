// createPlayer: the RendererHandle over a `Stage` (plan-a2 decisions 1 to 4). Pure: time comes
// from `options.clock`, frames from `Stage.onFrame`, and the scene from `sceneAt`.
import { validateManifest } from '../manifest/validate.ts';
import type { SpriteManifest } from '../manifest/types.ts';
import { prepareScene, reachedEnd, type PreparedScene } from '../scene/prepare.ts';
import { sceneAt } from '../scene/scene-at.ts';
import type { Scene } from '../scene/types.ts';
import { EMPTY_STATS, createFrameRecorder } from '../stats/frame-stats.ts';
import type { Cue, Timeline } from '../timeline/types.ts';
import { validateTimeline, type ManifestsById } from '../timeline/validate.ts';
import { cursorAt, dueCues, lastCueAt } from './cues.ts';
import { RendererError } from './errors.ts';
import {
  INITIAL_PLAYBACK,
  pause as pausePlayback,
  play as playPlayback,
  seek as seekPlayback,
  timeAt,
  type Playback,
} from './playback.ts';
import type {
  CueListener,
  PlayerState,
  RendererHandle,
  RendererOptions,
  RendererSnapshot,
  RendererTheme,
  Stage,
  StateListener,
} from './types.ts';

function validateAssets(assets: ReadonlyMap<string, unknown>): Map<string, SpriteManifest> {
  const manifests = new Map<string, SpriteManifest>();
  for (const [id, json] of assets) {
    const result = validateManifest(json, { expectedId: id });
    if (!result.ok) throw new RendererError('asset_invalid');
    manifests.set(id, result.manifest);
  }
  return manifests;
}

/** The fight of `loadLive`. `installed` once its sheets are in: until then `prepared` is older. */
interface LiveFight {
  timeline: Timeline;
  readonly manifests: ManifestsById;
  installed: boolean;
}

function sameCue(a: Cue, b: Cue): boolean {
  if (a.type !== b.type) return false;
  const left = a as unknown as Readonly<Record<string, unknown>>;
  const right = b as unknown as Readonly<Record<string, unknown>>;
  return Object.keys(left).every((key) => left[key] === right[key]);
}

const isClosed = (timeline: Timeline): boolean => {
  const last = timeline.cues.at(-1)?.type;
  return last === 'win' || last === 'draw';
};

/** Whether `next` can follow `previous` without touching the sheets nor what was played (RF47). */
function continues(previous: Timeline, next: Timeline): boolean {
  if (next.cues.length < previous.cues.length) return false;
  if (!previous.cues.every((cue, i) => sameCue(cue, next.cues[i] as Cue))) return false;
  const sameFighter = (side: 'a' | 'b'): boolean =>
    previous.fighters[side].asset === next.fighters[side].asset &&
    previous.fighters[side].max_hp === next.fighters[side].max_hp;
  if (!sameFighter('a') || !sameFighter('b')) return false;
  const closing = !isClosed(previous) && isClosed(next);
  return closing || previous.duration_ms === next.duration_ms;
}

export function createPlayer(options: RendererOptions, stage: Stage): RendererHandle {
  let reducedMotion = options.reducedMotion;
  let prepared: PreparedScene | null = null;
  let playback: Playback = INITIAL_PLAYBACK;
  let cursor = 0;
  let state: PlayerState | null = null;
  let scene: Scene | null = null;
  let destroyed = false;
  let loadId = 0;
  let live: LiveFight | null = null;
  /** Goes up when another timeline takes over; an `extend` keeps the indices, so it does not. */
  let installs = 0;
  const cueListeners = new Set<CueListener>();
  const stateListeners = new Set<StateListener>();
  const recorder = createFrameRecorder();

  const now = (): number => options.clock();

  const setState = (next: PlayerState): void => {
    if (state === next) return;
    state = next;
    for (const listener of [...stateListeners]) listener(next);
  };

  const settledState = (p: PreparedScene, t: number): PlayerState => {
    if (reachedEnd(p, t)) return 'finished';
    return playback.playing ? 'playing' : state === 'ready' ? 'ready' : 'paused';
  };

  function frame(elapsedMs: number): void {
    const p = prepared;
    if (destroyed || p === null) return;
    const clock = now();
    const t = timeAt(playback, clock, p.durationMs);
    // The scene first, so `snapshot()` inside a cue listener already shows that cue.
    render(p, t);
    if (!playback.playing) return;
    // Before the start, `t` is held at 0: the `intro` is not due yet (010 RF50).
    if (live?.installed === true && playback.anchorT + clock - playback.anchorClock < 0) return;
    // From the first strike to the closing cue (`win` or `draw`, 005 RF8).
    const closing = p.win ?? p.draw;
    if (p.firstStrikeAt !== null && t >= p.firstStrikeAt && (closing === null || t <= closing.at)) {
      recorder.record(elapsedMs);
    }
    const due = dueCues(p.cueTimes, cursor, t);
    cursor = due.to;
    const installsNow = installs;
    for (let i = due.from; i < due.to; i++) {
      const pc = p.cues[i];
      if (pc === undefined) continue;
      for (const listener of [...cueListeners]) listener(pc.cue, i);
      // A listener may pause, seek, destroy the handle or load another timeline. An `extend` only
      // adds cues at the end, so the rest of these still have to fire.
      if (destroyed || installs !== installsNow || cursor !== due.to) return;
    }
    if (playback.playing && reachedEnd(p, t)) setState('finished');
  }

  function render(p: PreparedScene, t: number): void {
    scene = sceneAt(p, t, { reducedMotion });
    stage.apply(scene);
  }

  const ready: Promise<void> = stage.init({
    theme: options.theme,
    labels: options.labels,
    formatNumber: options.formatNumber,
    onFrame: frame,
  });
  // Rejections surface through `load`; never as an unhandled rejection.
  ready.catch(() => undefined);

  function install(next: PreparedScene): void {
    prepared = next;
    installs++;
    playback = INITIAL_PLAYBACK;
    cursor = 0;
    recorder.reset();
    state = null;
    render(next, 0);
    setState('ready');
  }

  return {
    async load(timelineJson, assets) {
      const id = ++loadId;
      live = null;
      const manifests = validateAssets(assets);
      const result = validateTimeline(timelineJson, manifests);
      if (!result.ok) throw new RendererError('timeline_invalid');
      const next = prepareScene(result.timeline, manifests);
      await ready;
      if (destroyed || id !== loadId) return;
      await stage.loadAssets(manifests, options.resolveAsset);
      if (destroyed || id !== loadId) return;
      install(next);
    },
    async loadLive(timelineJson, assets) {
      const id = ++loadId;
      live = null;
      const manifests = validateAssets(assets);
      const result = validateTimeline(timelineJson, manifests, { partial: true });
      if (!result.ok) throw new RendererError('timeline_invalid');
      // Written now, so an `extend` that comes while the sheets load is not lost.
      const fight: LiveFight = { timeline: result.timeline, manifests, installed: false };
      live = fight;
      try {
        await ready;
        if (destroyed || id !== loadId) return;
        await stage.loadAssets(manifests, options.resolveAsset);
      } catch (error) {
        if (id === loadId) live = null;
        throw error;
      }
      if (destroyed || id !== loadId) return;
      const next = prepareScene(fight.timeline, manifests);
      fight.installed = true;
      install(next);
    },
    extend(timelineJson) {
      if (destroyed) return;
      const fight = live;
      if (fight === null) throw new RendererError('timeline_invalid');
      const result = validateTimeline(timelineJson, fight.manifests, { partial: true });
      if (!result.ok || !continues(fight.timeline, result.timeline)) {
        throw new RendererError('timeline_invalid');
      }
      const next = fight.installed ? prepareScene(result.timeline, fight.manifests) : null;
      fight.timeline = result.timeline;
      if (next !== null) prepared = next;
    },
    play() {
      const p = prepared;
      if (destroyed || p === null) return;
      const first = playback === INITIAL_PLAYBACK;
      playback = playPlayback(playback, now(), p.durationMs);
      const t = timeAt(playback, now(), p.durationMs);
      // Coming in late: what was skipped does not fire (010 RF50).
      if (first && live?.installed === true) cursor = cursorAt(p.cueTimes, t);
      setState(reachedEnd(p, t) ? 'finished' : 'playing');
    },
    pause() {
      const p = prepared;
      if (destroyed || p === null) return;
      playback = pausePlayback(playback, now(), p.durationMs);
      const t = timeAt(playback, now(), p.durationMs);
      setState(reachedEnd(p, t) ? 'finished' : 'paused');
    },
    seek(ms) {
      const p = prepared;
      if (destroyed || p === null) return;
      playback = seekPlayback(playback, now(), ms, p.durationMs);
      const t = timeAt(playback, now(), p.durationMs);
      cursor = cursorAt(p.cueTimes, t);
      recorder.reset();
      render(p, t);
      setState(settledState(p, t));
    },
    setTheme(theme: RendererTheme) {
      if (destroyed) return;
      stage.setTheme(theme);
      if (prepared !== null && scene !== null) stage.apply(scene);
    },
    setReducedMotion(flag) {
      reducedMotion = flag;
    },
    onCue(listener) {
      cueListeners.add(listener);
      return () => cueListeners.delete(listener);
    },
    onState(listener) {
      stateListeners.add(listener);
      return () => stateListeners.delete(listener);
    },
    snapshot(): RendererSnapshot | null {
      const p = prepared;
      if (p === null || scene === null) return null;
      const index = lastCueAt(p.cueTimes, scene.t);
      const pc = p.cues[index];
      return {
        t: scene.t,
        phase: scene.phase,
        winner: scene.winner,
        ending: scene.ending,
        hp: { a: scene.hud.a.hp, b: scene.hud.b.hp },
        lastCue: pc === undefined ? null : { index, type: pc.cue.type },
      };
    },
    getFrameStats() {
      return prepared === null ? EMPTY_STATS : recorder.stats();
    },
    destroy() {
      if (destroyed) return;
      destroyed = true;
      cueListeners.clear();
      stateListeners.clear();
      stage.destroy();
    },
  };
}
