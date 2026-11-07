<script lang="ts">
  // The controls of a replay (011 RF19): a row of 44 px that fits in 375 px. There is no bar, and +10 s
  // is never off before `finished`: either would tell how much is left (RF20).
  import ChevronLastIcon from '@lucide/svelte/icons/chevron-last';
  import PauseIcon from '@lucide/svelte/icons/pause';
  import PlayIcon from '@lucide/svelte/icons/play';
  import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
  import { Button } from '#lib/components/ui/button/index.js';
  import { m } from '#lib/paraglide/messages.js';
  import type { FightStageState } from '../fight/FightStage.svelte';

  interface Props {
    phase: FightStageState;
    ontoggle: () => void;
    onback: () => void;
    onforward: () => void;
    onrestart: () => void;
    onend: () => void;
  }

  let { phase, ontoggle, onback, onforward, onrestart, onend }: Props = $props();

  const busy = $derived(phase === 'loading' || phase === 'error');
  const finished = $derived(phase === 'finished');
  const toggleLabel = $derived(
    phase === 'playing'
      ? m.replay_control_pause()
      : finished
        ? m.replay_control_again()
        : m.replay_control_play(),
  );
</script>

<div
  role="group"
  aria-label={m.replay_controls_label()}
  class="grid grid-cols-5 gap-2 sm:flex sm:flex-wrap"
  data-testid="replay-controls"
>
  <Button
    class="w-full sm:w-11"
    size="icon"
    onclick={ontoggle}
    disabled={busy}
    aria-label={toggleLabel}
    title={toggleLabel}
    data-testid="replay-toggle"
    data-action={phase === 'playing' ? 'pause' : finished ? 'again' : 'play'}
  >
    {#if phase === 'playing'}
      <PauseIcon aria-hidden="true" />
    {:else if finished}
      <RotateCcwIcon aria-hidden="true" />
    {:else}
      <PlayIcon aria-hidden="true" />
    {/if}
  </Button>
  <Button
    class="w-full px-0 sm:w-14"
    size="icon"
    variant="outline"
    onclick={onback}
    disabled={busy}
    aria-label={m.replay_control_back()}
    title={m.replay_control_back()}
    data-testid="replay-back"
  >
    {m.replay_control_back_short()}
  </Button>
  <Button
    class="w-full px-0 sm:w-14"
    size="icon"
    variant="outline"
    onclick={onforward}
    disabled={busy || finished}
    aria-label={m.replay_control_forward()}
    title={m.replay_control_forward()}
    data-testid="replay-forward"
  >
    {m.replay_control_forward_short()}
  </Button>
  <Button
    class="w-full sm:w-11"
    size="icon"
    variant="outline"
    onclick={onrestart}
    disabled={busy}
    aria-label={m.replay_control_restart()}
    title={m.replay_control_restart()}
    data-testid="replay-restart"
  >
    <RotateCcwIcon aria-hidden="true" />
  </Button>
  <Button
    class="w-full sm:w-11"
    size="icon"
    variant="outline"
    onclick={onend}
    disabled={busy || finished}
    aria-label={m.replay_control_end()}
    title={m.replay_control_end()}
    data-testid="replay-end"
  >
    <ChevronLastIcon aria-hidden="true" />
  </Button>
</div>
