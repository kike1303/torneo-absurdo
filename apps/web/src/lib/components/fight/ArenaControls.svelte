<script lang="ts">
  // Controls of the test arena (RF-A16): play/pause, restart, a fighter per side and two
  // switches. Data by props; no renderer value here (ADR-0009 D4).
  import type { Side } from '@torneo/renderer';
  import { Button } from '#lib/components/ui/button/index.js';
  import { Label } from '#lib/components/ui/label/index.js';
  import type { ArenaFighterId } from '#lib/features/arena/fighters.js';
  import { m } from '#lib/paraglide/messages.js';
  import { fighterName } from './fighter-name.js';
  import type { FightStageState } from './FightStage.svelte';

  interface Props {
    phase: FightStageState;
    options: readonly ArenaFighterId[];
    fighters: Record<Side, ArenaFighterId>;
    reducedMotion: boolean;
    showStats: boolean;
    ontoggle: () => void;
    onrestart: () => void;
    /** Prefix of the element ids, for more than one instance on a page (the catalog). */
    idPrefix?: string;
  }

  let {
    phase,
    options,
    fighters = $bindable(),
    reducedMotion = $bindable(false),
    showStats = $bindable(false),
    ontoggle,
    onrestart,
    idPrefix = 'arena',
  }: Props = $props();

  const busy = $derived(phase === 'loading' || phase === 'error');
  const sides: { side: Side; label: () => string }[] = [
    { side: 'a', label: m.arena_side_a },
    { side: 'b', label: m.arena_side_b },
  ];
  const selectClass =
    'h-11 w-full rounded-lg border border-input bg-background px-3 text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background focus-visible:outline-none disabled:opacity-50';
</script>

<div class="grid grid-cols-2 gap-3 sm:flex sm:flex-wrap sm:items-end" data-testid="arena-controls">
  <Button onclick={ontoggle} disabled={busy} data-testid="arena-toggle">
    {phase === 'playing' ? m.arena_pause() : m.arena_play()}
  </Button>
  <Button variant="outline" onclick={onrestart} disabled={busy} data-testid="arena-restart">
    {m.arena_restart()}
  </Button>
  {#each sides as { side, label } (side)}
    <div class="flex flex-col gap-1.5 sm:w-44">
      <Label for={`${idPrefix}-fighter-${side}`}>{label()}</Label>
      <select
        id={`${idPrefix}-fighter-${side}`}
        class={selectClass}
        bind:value={fighters[side]}
        disabled={phase === 'loading'}
        data-testid={`${idPrefix}-fighter-${side}`}
      >
        {#each options as id (id)}
          <option value={id}>{fighterName(id)}</option>
        {/each}
      </select>
    </div>
  {/each}
  <Button
    variant={reducedMotion ? 'default' : 'outline'}
    role="switch"
    aria-checked={reducedMotion}
    onclick={() => (reducedMotion = !reducedMotion)}
    class="col-span-2 sm:col-span-1"
    data-testid="arena-reduced-motion"
  >
    {m.arena_reduced_motion()}
  </Button>
  <Button
    variant={showStats ? 'default' : 'outline'}
    role="switch"
    aria-checked={showStats}
    onclick={() => (showStats = !showStats)}
    class="col-span-2 sm:col-span-1"
    data-testid="arena-show-stats"
  >
    {m.arena_show_stats()}
  </Button>
</div>
