<script lang="ts" module>
  export type FightStageState = 'loading' | 'error' | 'ready' | 'playing' | 'paused' | 'finished';
</script>

<script lang="ts">
  // The only file that imports values from the renderer (ADR-0009 D4), and only with `import()`
  // on mount, so Pixi never reaches another chunk (RF-A16, RF-A17).
  import { onMount, untrack } from 'svelte';
  import type {
    FrameStats,
    RendererErrorCode,
    RendererHandle,
    RendererSnapshot,
    Side,
  } from '@torneo/renderer';
  import { Button } from '#lib/components/ui/button/index.js';
  import { Skeleton } from '#lib/components/ui/skeleton/index.js';
  import type { Timeline } from '@torneo/renderer/timeline';
  import type { ArenaFighterId, PlaceholderFighterId } from '#lib/features/arena/fighters.js';
  import type { LiveSession } from '#lib/features/fight/live-session.js';
  import type { LocalAssets } from '#lib/features/arena/local-fighters.js';
  import { formatInteger } from '#lib/i18n/format.js';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';
  import { theme } from '#lib/theme/theme.svelte.js';
  import { fighterName } from './fighter-name.js';
  import { startLive, type LiveRun } from './live-host.js';
  import { currentRendererTheme } from './renderer-theme.js';

  interface Props {
    fighters: Readonly<Record<Side, ArenaFighterId>>;
    /** The "simulate reduced motion" switch; OR-ed with `prefers-reduced-motion`. */
    reducedMotion?: boolean;
    /** Where the paused stage starts (`?t=` on /dev/arena); it plays only on `toggle` (TA-20). */
    initialT?: number;
    /** Exposes HP and the last cue in `data-*` for the e2e (only /dev/arena asks for it). */
    debug?: boolean;
    phase?: FightStageState;
    /** Loads the fighters that are not placeholders (`content/_local/sprites/`, only in dev). */
    local?: LocalAssets;
    /**
     * Rewrites the timeline before `load` (`?pace=` on /dev/arena). `isValid` is the renderer's
     * validator without manifests, so the caller never imports the renderer (006 RF15).
     */
    retime?: (timeline: Timeline, isValid: (timeline: Timeline) => boolean) => Timeline;
    /**
     * Where the timeline comes from, given the asset of each side (`?log=` on /dev/arena). Without
     * it, the spike script with these assets.
     */
    source?: (assets: Readonly<Record<Side, string>>) => Promise<Timeline>;
    /**
     * A fight in progress (010 RF47, RF50): it plays on its own by the clock of the session, with
     * no pause nor seek. `source`, `retime` and `initialT` do not apply.
     */
    live?: (assets: Readonly<Record<Side, string>>) => Promise<LiveSession>;
    /** The names to show; without it, those of the arena fighters. */
    displayNames?: Readonly<Record<Side, string>>;
    /** Shows the result, "replay" and the status under the stage (the arena). */
    showResult?: boolean;
    /** With `live`, every 250 ms: the way the host learns the result (010 plan-d decision 9). */
    onsnapshot?: (snapshot: RendererSnapshot | null) => void;
    /** With `live`: the first frame is on screen. */
    onpainted?: () => void;
    /** It will not draw: an error, or the WebGL context lost too often (plan-d decision 13). */
    onfailure?: (code: RendererErrorCode) => void;
  }

  let {
    fighters,
    reducedMotion = false,
    initialT = 0,
    debug = false,
    phase = $bindable('loading'),
    local,
    retime,
    source,
    live,
    displayNames,
    showResult = true,
    onsnapshot,
    onpainted,
    onfailure,
  }: Props = $props();

  const SNAPSHOT_MS = 250;
  /** More lost contexts than this in the window, and the canvas gives up (plan-d decision 13). */
  const MAX_CONTEXT_LOSSES = 2;
  const CONTEXT_LOSS_WINDOW_MS = 60_000;

  type Loader<T> = () => Promise<{ default: T }>;
  // Loaders by fighter id: the manifest JSON and the URL of each sheet (Vite hashes the PNG).
  const MANIFESTS: Record<PlaceholderFighterId, Loader<unknown>> = {
    dummy_a: () => import('@torneo/renderer/placeholders/dummy_a/manifest.json'),
    dummy_b: () => import('@torneo/renderer/placeholders/dummy_b/manifest.json'),
  };
  const SHEETS: Record<PlaceholderFighterId, Record<string, Loader<string>>> = {
    dummy_a: {
      'sheet-0.png': () => import('@torneo/renderer/placeholders/dummy_a/sheet-0.png?url'),
    },
    dummy_b: {
      'sheet-0.png': () => import('@torneo/renderer/placeholders/dummy_b/sheet-0.png?url'),
    },
  };

  const ERROR_MESSAGES: Record<RendererErrorCode, () => string> = {
    webgl_unavailable: m.arena_error_webgl_unavailable,
    asset_invalid: m.arena_error_asset_invalid,
    asset_load_failed: m.arena_error_asset_load_failed,
    timeline_invalid: m.arena_error_timeline_invalid,
  };

  let host = $state<HTMLDivElement | null>(null);
  let renderer: RendererHandle | null = null;
  let errorCode = $state<RendererErrorCode | null>(null);
  let snapshot = $state<RendererSnapshot | null>(null);
  let rendererLoaded = $state(false);
  let systemReduced = $state(false);
  let mounted = $state(false);
  let run = 0;
  let loadedTimeline: Timeline | null = null;
  let liveRun: LiveRun | null = null;
  let snapshotTimer: ReturnType<typeof setInterval> | undefined;
  let contextLosses: number[] = [];
  /** How long each `extend` took, in ms, for the stats JSON (010 plan-c decision 13). */
  let extendSamples: number[] = [];

  function isPlaceholder(id: ArenaFighterId): id is PlaceholderFighterId {
    return Object.hasOwn(MANIFESTS, id);
  }

  /** The manifest JSON of a fighter: a bundled placeholder or a local one (by props). */
  async function loadManifest(id: ArenaFighterId): Promise<unknown> {
    if (isPlaceholder(id)) return (await MANIFESTS[id]()).default;
    if (local === undefined)
      throw Object.assign(new Error('asset_load_failed'), { code: 'asset_load_failed' });
    return local.loadManifest(id);
  }

  function preloadSheets(ids: readonly ArenaFighterId[]): void {
    for (const fid of ids) {
      if (!isPlaceholder(fid)) continue;
      for (const load of Object.values(SHEETS[fid])) {
        // A failure here is not one: `loadLive` asks for them again and reports it.
        void load()
          .then(({ default: url }) => {
            new Image().src = url;
          })
          .catch(() => undefined);
      }
    }
  }

  /** The spike script with the assets of each side (the default `source`). */
  async function spikeTimeline(assets: Readonly<Record<Side, string>>): Promise<Timeline> {
    const spike = (await import('@torneo/renderer/fixtures/spike.timeline.json'))
      .default as unknown as Timeline;
    return {
      ...spike,
      fighters: {
        a: { ...spike.fighters.a, asset: assets.a },
        b: { ...spike.fighters.b, asset: assets.b },
      },
    };
  }

  const names = $derived(
    displayNames ?? { a: fighterName(fighters.a), b: fighterName(fighters.b) },
  );
  const winnerName = $derived(snapshot?.winner ? names[snapshot.winner] : null);
  /** The result text in `finished`, by `ending` (005 RF16): KO, decision or draw. */
  const result = $derived.by((): { testid: string; text: string } | null => {
    const ending = snapshot?.ending ?? null;
    if (ending === 'draw') return { testid: 'fight-draw', text: m.arena_draw() };
    if (winnerName === null) return null;
    const text =
      ending === 'decision'
        ? m.arena_winner_decision({ name: winnerName })
        : m.arena_winner({ name: winnerName });
    return { testid: 'fight-winner', text };
  });
  const motionReduced = $derived(systemReduced || reducedMotion);

  function refresh(): void {
    snapshot = renderer?.snapshot() ?? null;
  }

  function cancelLive(): void {
    liveRun?.cancel();
    liveRun = null;
    clearInterval(snapshotTimer);
    snapshotTimer = undefined;
  }

  function watchSnapshots(): void {
    const read = (): void => {
      refresh();
      onsnapshot?.(snapshot);
      if (snapshot?.phase === 'finished') {
        clearInterval(snapshotTimer);
        snapshotTimer = undefined;
      }
    };
    read();
    snapshotTimer = setInterval(read, SNAPSHOT_MS);
  }

  /** The scene comes from `t`: a new renderer with the same session is all it takes. */
  function watchContext(container: HTMLElement): void {
    container.querySelector('canvas')?.addEventListener(
      'webglcontextlost',
      (event) => {
        event.preventDefault();
        const now = performance.now();
        contextLosses = [...contextLosses.filter((at) => now - at < CONTEXT_LOSS_WINDOW_MS), now];
        if (contextLosses.length >= MAX_CONTEXT_LOSSES) {
          fail(Object.assign(new Error('webgl_unavailable'), { code: 'webgl_unavailable' }));
        } else {
          void start();
        }
      },
      { once: true },
    );
  }

  function fail(error: unknown): void {
    cancelLive();
    renderer?.destroy();
    renderer = null;
    const code = (error as { code?: unknown } | null)?.code;
    errorCode =
      typeof code === 'string' && code in ERROR_MESSAGES
        ? (code as RendererErrorCode)
        : 'asset_load_failed';
    phase = 'error';
    onfailure?.(errorCode);
  }

  async function start(): Promise<void> {
    const id = ++run;
    cancelLive();
    extendSamples = [];
    renderer?.destroy();
    renderer = null;
    phase = 'loading';
    errorCode = null;
    snapshot = null;
    loadedTimeline = null;
    const container = host;
    if (container === null) return;
    const side = { a: fighters.a, b: fighters.b };
    try {
      const ids = [...new Set([side.a, side.b])];
      const opening = live?.(side);
      // The events only come at the start, so the sheets are in the cache before them: with a slow
      // network the first frame would not arrive in time (010 plan-d decision 14).
      if (opening !== undefined) preloadSheets(ids);
      const [rendererModule, cast, ...manifestModules] = await Promise.all([
        import('@torneo/renderer'),
        opening === undefined
          ? (source ?? spikeTimeline)(side)
          : opening.then((session) => session.timeline),
        ...ids.map((fid) => loadManifest(fid)),
      ]);
      const urls = (
        await Promise.all(
          ids.flatMap((fid) =>
            isPlaceholder(fid)
              ? Object.entries(SHEETS[fid]).map(async ([file, load]) => ({
                  fid,
                  file,
                  url: (await load()).default,
                }))
              : [],
          ),
        )
      ).filter((entry) => entry.url !== '');
      if (id !== run) return;
      rendererLoaded = true;
      const assets = new Map<string, unknown>(ids.map((fid, i) => [fid, manifestModules[i]]));
      const session = opening === undefined ? null : await opening;
      const timeline =
        session !== null || retime === undefined
          ? cast
          : retime(cast, (t) => rendererModule.validateTimeline(t).ok);
      loadedTimeline = timeline;
      const start = performance.now();
      const locale = getLocale();
      const handle = rendererModule.createRenderer(container, {
        theme: currentRendererTheme(),
        reducedMotion: motionReduced,
        clock: session?.clock ?? (() => performance.now() - start),
        labels: {
          names,
          banners: {
            fight: m.arena_banner_fight(),
            ko: m.arena_banner_ko(),
            time: m.arena_banner_time(),
          },
        },
        formatNumber: (n) => formatInteger(n, locale),
        resolveAsset: (assetId, file) =>
          isPlaceholder(assetId)
            ? (urls.find((u) => u.fid === assetId && u.file === file)?.url ?? '')
            : (local?.sheetUrl(assetId, file) ?? ''),
      });
      renderer = handle;
      // Before `load`, so its `ready` reaches `phase`; a superseded handle never writes it.
      handle.onState((next) => {
        if (id !== run) return;
        phase = next;
        refresh();
      });
      if (session !== null) {
        const liveNow = startLive({
          handle,
          session,
          timeline,
          assets,
          now: () => performance.now(),
          onExtended: (next, ms) => {
            extendSamples.push(ms);
            loadedTimeline = next;
          },
          beforePlay: () => {
            if (debug) handle.onCue(refresh);
          },
          onFail: (error) => {
            if (id === run) fail(error);
          },
        });
        liveRun = liveNow;
        const playing = await liveNow.done;
        if (id !== run) {
          handle.destroy();
          return;
        }
        if (!playing) return;
        watchSnapshots();
        watchContext(container);
        // Two frames: the one already asked for and the one that shows it.
        requestAnimationFrame(() =>
          requestAnimationFrame(() => {
            if (id === run) onpainted?.();
          }),
        );
        return;
      }
      await handle.load(timeline, assets);
      if (id !== run) {
        // Superseded by a newer start (a fighter switched quickly): never leave it running.
        handle.destroy();
        return;
      }
      if (debug) handle.onCue(refresh);
      // Paused at `initialT` (TA-20): a load, or a change of fighters, never starts playback.
      handle.seek(initialT);
      refresh();
    } catch (error) {
      if (id !== run) return;
      fail(error);
    }
  }

  export function toggle(): void {
    // A pause would break `t = clock()`, and live playback has no way back to it.
    if (renderer === null || live !== undefined) return;
    if (phase === 'playing') renderer.pause();
    else if (phase === 'finished') restart();
    else renderer.play();
  }

  export function restart(): void {
    if (renderer === null) return;
    if (live !== undefined) {
      void start();
      return;
    }
    renderer.seek(0);
    renderer.play();
    refresh();
  }

  export function getFrameStats(): FrameStats | null {
    return renderer?.getFrameStats() ?? null;
  }

  /** How long each `extend` took so far, in ms, for the stats JSON. */
  export function extendDurations(): readonly number[] {
    return extendSamples;
  }

  /** The timeline given to the renderer (after `retime`), for the stats JSON. */
  export function currentTimeline(): Timeline | null {
    return loadedTimeline;
  }

  /** Width of the drawing surface in CSS px, for the stats JSON. */
  export function canvasWidth(): number {
    return host?.clientWidth ?? 0;
  }

  onMount(() => {
    const query = matchMedia('(prefers-reduced-motion: reduce)');
    systemReduced = query.matches;
    const onChange = (event: MediaQueryListEvent) => (systemReduced = event.matches);
    query.addEventListener('change', onChange);
    mounted = true;
    return () => {
      query.removeEventListener('change', onChange);
      run++;
      cancelLive();
      renderer?.destroy();
      renderer = null;
    };
  });

  // A change of fighters reloads (paused): names and assets belong to the renderer instance.
  $effect(() => {
    if (!mounted) return;
    void fighters.a;
    void fighters.b;
    void start();
  });

  $effect(() => {
    renderer?.setReducedMotion(motionReduced);
  });

  // ADR-0009 D7: rebuild the theme when the resolved theme changes.
  $effect(() => {
    void theme.resolved;
    untrack(() => renderer?.setTheme(currentRendererTheme()));
  });
</script>

<div class="flex flex-col gap-3">
  <div
    class="relative aspect-video w-full overflow-hidden rounded-md bg-arena-bg"
    data-testid="fight-stage"
    data-state={phase}
    data-error={errorCode ?? undefined}
    data-renderer-loaded={rendererLoaded ? '' : undefined}
    data-hp-a={debug ? snapshot?.hp.a : undefined}
    data-hp-b={debug ? snapshot?.hp.b : undefined}
    data-last-cue={debug ? snapshot?.lastCue?.index : undefined}
    data-ending={debug ? (snapshot?.ending ?? undefined) : undefined}
  >
    <div
      bind:this={host}
      class="absolute inset-0"
      role="img"
      aria-label={m.arena_canvas_label({ a: names.a, b: names.b })}
    ></div>
    {#if phase === 'loading'}
      <Skeleton class="absolute inset-0 rounded-md" aria-hidden="true" />
    {/if}
  </div>

  {#if showResult}
    <div aria-live="polite" class="flex flex-wrap items-center gap-3 empty:hidden">
      {#if phase === 'loading'}
        <p class="text-sm text-muted-foreground">{m.arena_loading()}</p>
      {:else if phase === 'error' && errorCode !== null}
        <p class="text-sm" data-testid="fight-error">{ERROR_MESSAGES[errorCode]()}</p>
        <Button variant="outline" onclick={() => void start()}>{m.arena_retry()}</Button>
      {:else if phase === 'finished' && result !== null}
        <p class="font-display text-lg font-semibold" data-testid={result.testid}>
          {result.text}
        </p>
        <Button variant="outline" onclick={restart}>{m.arena_replay()}</Button>
      {/if}
    </div>
  {/if}
</div>
