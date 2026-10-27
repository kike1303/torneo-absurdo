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
  import type { LocalAssets } from '#lib/features/arena/local-fighters.js';
  import { formatInteger } from '#lib/i18n/format.js';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';
  import { theme } from '#lib/theme/theme.svelte.js';
  import { fighterName } from './fighter-name.js';
  import { currentRendererTheme } from './renderer-theme.js';

  interface Props {
    fighters: Readonly<Record<Side, ArenaFighterId>>;
    /** The "simulate reduced motion" switch; OR-ed with `prefers-reduced-motion`. */
    reducedMotion?: boolean;
    /** Where playback starts (`?t=` on /dev/arena). */
    initialT?: number;
    /** Exposes HP and the last cue in `data-*` for the e2e (only /dev/arena asks for it). */
    debug?: boolean;
    phase?: FightStageState;
    /** Loads the fighters that are not placeholders (`content/_local/sprites/`, only in dev). */
    local?: LocalAssets;
    /** Rewrites the timeline before `load` (`?pace=` on /dev/arena). */
    retime?: (timeline: Timeline) => Timeline;
    /**
     * Where the timeline comes from, given the asset of each side (`?log=` on /dev/arena). Without
     * it, the spike script with these assets.
     */
    source?: (assets: Readonly<Record<Side, string>>) => Promise<Timeline>;
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
  }: Props = $props();

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

  const names = $derived({ a: fighterName(fighters.a), b: fighterName(fighters.b) });
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

  async function start(): Promise<void> {
    const id = ++run;
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
      const [rendererModule, cast, ...manifestModules] = await Promise.all([
        import('@torneo/renderer'),
        (source ?? spikeTimeline)(side),
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
      const timeline = retime === undefined ? cast : retime(cast);
      loadedTimeline = timeline;
      const start = performance.now();
      const locale = getLocale();
      const handle = rendererModule.createRenderer(container, {
        theme: currentRendererTheme(),
        reducedMotion: motionReduced,
        clock: () => performance.now() - start,
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
      await handle.load(timeline, assets);
      if (id !== run) {
        // Superseded by a newer start (a fighter switched quickly): never leave it running.
        handle.destroy();
        return;
      }
      handle.onState((next) => {
        phase = next;
        refresh();
      });
      if (debug) handle.onCue(refresh);
      handle.seek(initialT);
      handle.play();
      refresh();
    } catch (error) {
      if (id !== run) return;
      renderer?.destroy();
      renderer = null;
      const code = (error as { code?: unknown } | null)?.code;
      errorCode =
        typeof code === 'string' && code in ERROR_MESSAGES
          ? (code as RendererErrorCode)
          : 'asset_load_failed';
      phase = 'error';
    }
  }

  export function toggle(): void {
    if (renderer === null) return;
    if (phase === 'playing') renderer.pause();
    else if (phase === 'finished') restart();
    else renderer.play();
  }

  export function restart(): void {
    if (renderer === null) return;
    renderer.seek(0);
    renderer.play();
    refresh();
  }

  export function getFrameStats(): FrameStats | null {
    return renderer?.getFrameStats() ?? null;
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
      renderer?.destroy();
      renderer = null;
    };
  });

  // A change of fighters starts over: names and assets belong to the renderer instance.
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
</div>
