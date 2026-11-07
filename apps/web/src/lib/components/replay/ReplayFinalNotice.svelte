<script lang="ts">
  // A fight that has nothing to play (011 RF22): it says why, without looking like an error. A reason
  // this build does not know shows "voided" without a motive.
  import type { FightFinal } from '@torneo/protocol';
  import * as Card from '#lib/components/ui/card/index.js';
  import { m } from '#lib/paraglide/messages.js';

  let { final }: { final: FightFinal } = $props();

  const text = $derived.by((): string | null => {
    const reason: string = final.reason;
    if (final.kind === 'cancelled') {
      if (reason === 'missed_window') return m.replay_final_missed_window();
      return reason === 'fight_unrunnable' ? m.replay_final_fight_unrunnable() : null;
    }
    if (final.kind === 'voided') {
      if (reason === 'snapshot_invalid') return m.replay_final_snapshot_invalid();
      if (reason === 'secret_lost') return m.replay_final_secret_lost();
      return reason === 'engine_error' ? m.replay_final_engine_error() : m.replay_final_voided();
    }
    return null;
  });
</script>

{#if text !== null}
  <Card.Root data-testid="replay-final" data-kind={final.kind} data-reason={final.reason}>
    <Card.Content>
      <p class="font-display text-lg font-semibold">{text}</p>
    </Card.Content>
  </Card.Root>
{/if}
