<script lang="ts">
  // How a fight ended (010 RF54): the fight just seen, or the previous one next to what comes.
  import { Button } from '#lib/components/ui/button/index.js';
  import * as Card from '#lib/components/ui/card/index.js';
  import type { FighterDisplay } from '#lib/features/fight/live/fighter-names.js';
  import type { FightResult } from '#lib/features/fight/live/view-state.js';
  import type { VoidStats } from '#lib/features/fight/live/void-stats.js';
  import { formatInteger } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';

  interface Props {
    result: FightResult;
    /** With the label of the previous fight. */
    previous: boolean;
    display: (slug: string, locale: Locale) => FighterDisplay;
    locale: Locale;
    /** The replay of this fight: only once it is settled, and never on the replay itself (011 RF26). */
    replayHref?: string;
    /** The public counter (012 RF23), with a fight that cannot be verified. */
    voidStats?: VoidStats | null;
  }

  let { result, previous, display, locale, replayHref, voidStats = null }: Props = $props();

  const nameA = $derived(display(result.fight.a.slug, locale).name);
  const nameB = $derived(display(result.fight.b.slug, locale).name);
  const winner = $derived(
    result.winner === null ? '' : display(result.fight[result.winner].slug, locale).name,
  );
  const text = $derived.by(() => {
    switch (result.kind) {
      case 'ko':
        return m.fight_result_ko({ name: winner });
      case 'decision':
        return m.fight_result_decision({ name: winner });
      case 'draw':
        return m.fight_result_draw();
      case 'cancelled':
        return m.fight_result_cancelled();
      case 'voided':
        return m.fight_result_voided();
      case 'voided_unverifiable':
        return m.fight_result_voided_unverifiable();
    }
  });
</script>

<Card.Root data-testid="fight-result" data-result={result.kind}>
  <Card.Content class="flex flex-col gap-1">
    {#if previous}
      <h2 class="text-sm font-medium text-muted-foreground">{m.fight_view_last()}</h2>
    {/if}
    <p class="text-sm text-muted-foreground">{nameA} {m.fight_view_versus()} {nameB}</p>
    <p class="font-display text-lg font-semibold" data-testid="fight-result-text">{text}</p>
    {#if result.kind === 'voided_unverifiable' && voidStats !== null}
      <p class="text-sm text-muted-foreground" data-testid="fight-result-void-count">
        {m.fight_result_void_count({
          voided: formatInteger(voidStats.voided, locale),
          secret_lost: formatInteger(voidStats.secret_lost, locale),
        })}
      </p>
    {/if}
    {#if replayHref !== undefined}
      <div class="pt-1">
        <Button variant="outline" href={replayHref} data-testid="fight-result-replay">
          {m.replay_result_link()}
        </Button>
      </div>
    {/if}
  </Card.Content>
</Card.Root>
