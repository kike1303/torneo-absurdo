<script lang="ts">
  // My botarga in 3D (021 "Web"): the stage of the arena with the showcase, paused in its idle at
  // the start; "see the victory" plays to its victory. The idle and the victory are the ones it
  // picked; the sparring partner wears the tournament ones.
  import type { FighterCosmetics } from '@torneo/protocol';
  import type { AdaptedTimeline } from '#lib/features/fight/timeline-from-fight-log.js';
  import { Button } from '#lib/components/ui/button/index.js';
  import { m } from '#lib/paraglide/messages.js';
  import FightNotice from '../fight/FightNotice.svelte';
  import FightStage, { type FightStageState } from '../fight/FightStage.svelte';

  interface Props {
    name: string;
    cosmetics: FighterCosmetics;
    /** The accessories it wears, by catalog id (their 3D model in content/models/accessories). */
    wear: readonly string[];
    /** The showcase timeline (`features/botarga/showcase.ts`). */
    source: () => Promise<AdaptedTimeline>;
  }

  let { name, cosmetics, wear, source }: Props = $props();

  let stage = $state<FightStage | null>(null);
  let phase = $state<FightStageState>('loading');
  let failed = $state<string | null>(null);

  // The renderer reads `cosmetics` and `looks` once, on mount; the page keys this by them.
  const picked = $derived({
    a: {
      ...(cosmetics.idle === null ? {} : { idle: cosmetics.idle }),
      ...(cosmetics.victory === null ? {} : { victory: cosmetics.victory }),
    },
  });
</script>

<div class="flex flex-col gap-3" data-testid="botarga-preview" data-phase={phase}>
  {#if failed === 'webgl_unavailable'}
    <FightNotice text={m.fight_notice_no_animation()} testid="fight-notice-no-animation" />
  {:else if failed !== null}
    <FightNotice text={m.replay_notice_playback_error()} testid="replay-notice-playback" />
  {:else}
    <FightStage
      bind:this={stage}
      bind:phase
      fighters={{ a: 'dummy_a', b: 'dummy_b' }}
      displayNames={{ a: name, b: m.botarga_sparring() }}
      cosmetics={picked}
      looks={wear.length === 0 ? undefined : { a: { wear } }}
      {source}
      showResult={false}
      onfailure={(code) => (failed = code)}
    />
    <div>
      <Button
        variant="outline"
        class="min-h-11"
        disabled={phase === 'loading' || phase === 'error'}
        onclick={() => (phase === 'finished' ? stage?.restart() : stage?.toggle())}
        data-testid="botarga-preview-play"
      >
        {phase === 'playing' ? m.replay_control_pause() : m.botarga_preview_play()}
      </Button>
    </div>
  {/if}
</div>
