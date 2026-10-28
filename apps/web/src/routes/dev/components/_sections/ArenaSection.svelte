<script lang="ts">
  // Arena controls and stats with sample data (002 plan-a2 step 8). FightStage is not here: it
  // would load Pixi and WebGL in the catalog; its page is /dev/arena.
  import type { FrameStats, Side } from '@torneo/renderer';
  import ArenaControls from '#lib/components/fight/ArenaControls.svelte';
  import ArenaStats from '#lib/components/fight/ArenaStats.svelte';
  import { ARENA_FIGHTERS, type ArenaFighterId } from '#lib/features/arena/fighters.js';
  import type { PaceResolution } from '#lib/features/arena/retime.js';
  import Section from './Section.svelte';

  const SAMPLE: FrameStats = {
    frames: 3482,
    fpsMean: 59.6,
    p50: 16.7,
    p95: 17.4,
    p99: 21.3,
    over33: 2,
  };

  /** `?pace=0.5` on a timeline whose minimum is 0.86 (006 RF16): the row shows the clamp note. */
  const PACE: PaceResolution = { requested: 0.5, min: 0.86, effective: 0.86, clamped: true };

  let fighters = $state<Record<Side, ArenaFighterId>>({ a: 'dummy_a', b: 'dummy_b' });
  let reducedMotion = $state(false);
  let showStats = $state(true);
</script>

<Section title="Arena controls" id="arena-controls">
  <ArenaControls
    phase="paused"
    options={ARENA_FIGHTERS}
    localOptions={['local_1', 'local_2']}
    bind:fighters
    bind:reducedMotion
    bind:showStats
    ontoggle={() => {}}
    onrestart={() => {}}
  />
  <ArenaControls
    phase="loading"
    idPrefix="arena-loading"
    options={ARENA_FIGHTERS}
    fighters={{ a: 'dummy_a', b: 'dummy_b' }}
    reducedMotion={false}
    showStats={false}
    ontoggle={() => {}}
    onrestart={() => {}}
  />
</Section>

<Section title="Arena stats" id="arena-stats">
  <ArenaStats
    stats={SAMPLE}
    context={{
      devicePixelRatio: 2,
      canvasWidth: 343,
      fighters: ['dummy_a', 'dummy_b'],
      mean_strike_gap_ms: 1050,
    }}
    pace={PACE}
  />
</Section>
