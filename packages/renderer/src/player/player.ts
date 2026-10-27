// createPlayer: the RendererHandle over a `Stage` (plan-a2 decisions 1 to 4). Pure: time comes
// from `options.clock`, frames from `Stage.onFrame`, and the scene from `sceneAt`.
import { validateManifest } from '../manifest/validate.ts';
import type { SpriteManifest } from '../manifest/types.ts';
import { prepareScene, type PreparedScene } from '../scene/prepare.ts';
import { sceneAt } from '../scene/scene-at.ts';
import type { Scene } from '../scene/types.ts';
import { EMPTY_STATS, createFrameRecorder } from '../stats/frame-stats.ts';
import { validateTimeline } from '../timeline/validate.ts';
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

export function createPlayer(options: RendererOptions, stage: Stage): RendererHandle {
  let reducedMotion = options.reducedMotion;
  let prepared: PreparedScene | null = null;
  let playback: Playback = INITIAL_PLAYBACK;
  let cursor = 0;
  let state: PlayerState | null = null;
  let scene: Scene | null = null;
  let destroyed = false;
  let loadId = 0;
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
    if (t >= p.durationMs) return 'finished';
    return playback.playing ? 'playing' : state === 'ready' ? 'ready' : 'paused';
  };

  function frame(elapsedMs: number): void {
    const p = prepared;
    if (destroyed || p === null) return;
    const t = timeAt(playback, now(), p.durationMs);
    // The scene first, so `snapshot()` inside a cue listener already shows that cue.
    render(p, t);
    if (!playback.playing) return;
    // From the first strike to the closing cue (`win` or `draw`, 005 RF8).
    const closing = p.win ?? p.draw;
    if (p.firstStrikeAt !== null && t >= p.firstStrikeAt && (closing === null || t <= closing.at)) {
      recorder.record(elapsedMs);
    }
    const due = dueCues(p.cueTimes, cursor, t);
    cursor = due.to;
    for (let i = due.from; i < due.to; i++) {
      const pc = p.cues[i];
      if (pc === undefined) continue;
      for (const listener of [...cueListeners]) listener(pc.cue, i);
      // A listener may pause, seek, destroy the handle or load another timeline.
      if (destroyed || prepared !== p || cursor !== due.to) return;
    }
    if (playback.playing && t >= p.durationMs) setState('finished');
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

  return {
    async load(timelineJson, assets) {
      const id = ++loadId;
      const manifests = validateAssets(assets);
      const result = validateTimeline(timelineJson, manifests);
      if (!result.ok) throw new RendererError('timeline_invalid');
      const next = prepareScene(result.timeline, manifests);
      await ready;
      if (destroyed || id !== loadId) return;
      await stage.loadAssets(manifests, options.resolveAsset);
      if (destroyed || id !== loadId) return;
      prepared = next;
      playback = INITIAL_PLAYBACK;
      cursor = 0;
      recorder.reset();
      state = null;
      render(next, 0);
      setState('ready');
    },
    play() {
      const p = prepared;
      if (destroyed || p === null) return;
      playback = playPlayback(playback, now(), p.durationMs);
      const t = timeAt(playback, now(), p.durationMs);
      setState(t >= p.durationMs ? 'finished' : 'playing');
    },
    pause() {
      const p = prepared;
      if (destroyed || p === null) return;
      playback = pausePlayback(playback, now(), p.durationMs);
      const t = timeAt(playback, now(), p.durationMs);
      setState(t >= p.durationMs ? 'finished' : 'paused');
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
