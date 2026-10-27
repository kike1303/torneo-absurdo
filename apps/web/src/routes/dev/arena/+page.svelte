<script lang="ts">
  // Test arena of 002 (RF-A16), behind the /dev/** guard. It only composes: every visible text
  // lives in lib/components/fight/ (ADR-0009 D5). Phase B adds the local fighters (only in
  // `vite dev`) and `?pace=` (RF-B7, RF-B9); phase C, `?log=` to play an engine log (RF-C3).
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import type { FrameStats, Side } from '@torneo/renderer';
  import type { Timeline } from '@torneo/renderer/timeline';
  import ArenaControls from '#lib/components/fight/ArenaControls.svelte';
  import ArenaStats from '#lib/components/fight/ArenaStats.svelte';
  import FightStage, { type FightStageState } from '#lib/components/fight/FightStage.svelte';
  import { ARENA_FIGHTERS, type ArenaFighterId } from '#lib/features/arena/fighters.js';
  import { createLocalAssets, loadLocalFighterIds } from '#lib/features/arena/local-fighters.js';
  import { loadEngineLogTimeline, parseEngineLog } from '#lib/features/fight/engine-logs.js';
  import { meanStrikeGapMs, parsePace, retimeTimeline } from '#lib/features/arena/retime.js';
  import { m } from '#lib/paraglide/messages.js';

  const STATS_REFRESH_MS = 500;
  const EMPTY: FrameStats = { frames: 0, fpsMean: 0, p50: 0, p95: 0, p99: 0, over33: 0 };

  /** `?t=<ms>`: any non-negative integer (the renderer clamps it to the duration), else 0. */
  function initialT(): number {
    const raw = page.url.searchParams.get('t');
    return raw !== null && /^\d+$/.test(raw) ? Number(raw) : 0;
  }

  let stage = $state<FightStage | null>(null);
  let phase = $state<FightStageState>('loading');
  let fighters = $state<Record<Side, ArenaFighterId>>({ a: 'dummy_a', b: 'dummy_b' });
  let localIds = $state<string[]>([]);
  let reducedMotion = $state(false);
  let showStats = $state(false);
  let stats = $state<FrameStats>(EMPTY);
  let canvasWidth = $state(0);
  let meanGap = $state<number | null>(null);
  let dpr = $state(1);
  const start = initialT();
  const pace = parsePace(page.url.searchParams.get('pace'));
  const retime = pace === null ? undefined : (t: Timeline) => retimeTimeline(t, pace);
  const log = parseEngineLog(page.url.searchParams.get('log'));
  const source =
    log === null
      ? undefined
      : (assets: Readonly<Record<Side, string>>) => loadEngineLogTimeline(log, assets);
  const local = createLocalAssets((url) => fetch(url));

  onMount(() => {
    dpr = devicePixelRatio;
    void loadLocalFighterIds((url) => fetch(url)).then((ids) => (localIds = ids));
  });

  $effect(() => {
    if (!showStats) return;
    const read = () => {
      stats = stage?.getFrameStats() ?? EMPTY;
      canvasWidth = stage?.canvasWidth() ?? 0;
      const timeline = stage?.currentTimeline() ?? null;
      meanGap = timeline === null ? null : meanStrikeGapMs(timeline);
    };
    read();
    const timer = setInterval(read, STATS_REFRESH_MS);
    return () => clearInterval(timer);
  });
</script>

<svelte:head>
  <title>{m.arena_title()}</title>
</svelte:head>

<div class="flex flex-col gap-4" data-testid="arena">
  <FightStage
    bind:this={stage}
    bind:phase
    fighters={{ a: fighters.a, b: fighters.b }}
    {reducedMotion}
    initialT={start}
    {local}
    {retime}
    {source}
    debug
  />
  <ArenaControls
    {phase}
    options={ARENA_FIGHTERS}
    localOptions={localIds}
    bind:fighters
    bind:reducedMotion
    bind:showStats
    ontoggle={() => stage?.toggle()}
    onrestart={() => stage?.restart()}
  />
  {#if showStats}
    <ArenaStats
      {stats}
      context={{
        devicePixelRatio: dpr,
        canvasWidth,
        fighters: [fighters.a, fighters.b],
        mean_strike_gap_ms: meanGap,
      }}
    />
  {/if}
</div>
