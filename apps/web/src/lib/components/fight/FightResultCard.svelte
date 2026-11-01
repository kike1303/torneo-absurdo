<script lang="ts">
  // How a fight ended (010 RF54): the fight just seen, or the previous one next to what comes.
  import * as Card from '#lib/components/ui/card/index.js';
  import type { FighterDisplay } from '#lib/features/fight/live/fighter-names.js';
  import type { FightResult } from '#lib/features/fight/live/view-state.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';

  interface Props {
    result: FightResult;
    /** With the label of the previous fight. */
    previous: boolean;
    display: (slug: string, locale: Locale) => FighterDisplay;
    locale: Locale;
  }

  let { result, previous, display, locale }: Props = $props();

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
  </Card.Content>
</Card.Root>
