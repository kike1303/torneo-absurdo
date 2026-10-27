<script lang="ts">
  // Test arena of 002 (RF-A16), behind the /dev/** guard. It only composes: every visible text
  // lives in lib/components/fight/ (ADR-0009 D5).
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import type { FrameStats, Side } from '@torneo/renderer';
  import ArenaControls from '#lib/components/fight/ArenaControls.svelte';
  import ArenaStats from '#lib/components/fight/ArenaStats.svelte';
  import FightStage, { type FightStageState } from '#lib/components/fight/FightStage.svelte';
  import { ARENA_FIGHTERS, type ArenaFighterId } from '#lib/features/arena/fighters.js';
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
  let reducedMotion = $state(false);
  let showStats = $state(false);
  let stats = $state<FrameStats>(EMPTY);
  let canvasWidth = $state(0);
  let dpr = $state(1);
  const start = initialT();

  onMount(() => {
    dpr = devicePixelRatio;
  });

  $effect(() => {
    if (!showStats) return;
    const read = () => {
      stats = stage?.getFrameStats() ?? EMPTY;
      canvasWidth = stage?.canvasWidth() ?? 0;
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
    debug
  />
  <ArenaControls
    {phase}
    options={ARENA_FIGHTERS}
    bind:fighters
    bind:reducedMotion
    bind:showStats
    ontoggle={() => stage?.toggle()}
    onrestart={() => stage?.restart()}
  />
  {#if showStats}
    <ArenaStats
      {stats}
      context={{ devicePixelRatio: dpr, canvasWidth, fighters: [fighters.a, fighters.b] }}
    />
  {/if}
</div>
