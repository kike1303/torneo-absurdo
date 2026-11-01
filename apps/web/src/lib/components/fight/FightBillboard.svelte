<script lang="ts">
  // Who fights next and when (010 RF54). A column: there are epithets of 40 characters, and the card
  // fits whole in 375×667.
  import type { FightSummary } from '@torneo/protocol';
  import * as Card from '#lib/components/ui/card/index.js';
  import type { FighterDisplay } from '#lib/features/fight/live/fighter-names.js';
  import { formatClock } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';
  import FightCommit from './FightCommit.svelte';

  interface Props {
    fight: FightSummary;
    aboutToStart: boolean;
    remainingMs: number | null;
    display: (slug: string, locale: Locale) => FighterDisplay;
    locale: Locale;
  }

  let { fight, aboutToStart, remainingMs, display, locale }: Props = $props();

  const a = $derived(display(fight.a.slug, locale));
  const b = $derived(display(fight.b.slug, locale));
</script>

<Card.Root data-testid="fight-billboard">
  <Card.Content class="flex flex-col gap-2">
    <h2 class="text-sm font-medium text-muted-foreground">{m.fight_view_next()}</h2>
    <div class="flex flex-col gap-1">
      <div>
        <p class="font-display text-lg font-semibold" data-testid="fight-name-a">{a.name}</p>
        {#if a.epithet !== null}
          <p class="text-sm text-muted-foreground">{a.epithet}</p>
        {/if}
      </div>
      <p class="text-sm text-muted-foreground">{m.fight_view_versus()}</p>
      <div>
        <p class="font-display text-lg font-semibold" data-testid="fight-name-b">{b.name}</p>
        {#if b.epithet !== null}
          <p class="text-sm text-muted-foreground">{b.epithet}</p>
        {/if}
      </div>
    </div>
    {#if aboutToStart}
      <p class="font-display text-xl font-semibold" data-testid="fight-about-to-start">
        {m.fight_view_about_to_start()}
      </p>
    {:else if remainingMs !== null}
      <!-- It changes every second: never announced (RF58). -->
      <p class="font-display text-xl font-semibold tabular-nums" data-testid="fight-countdown">
        {m.fight_view_countdown({ time: formatClock(remainingMs, locale) })}
      </p>
    {/if}
    {#if fight.seed_commit !== null}
      <FightCommit commit={fight.seed_commit} />
    {/if}
  </Card.Content>
</Card.Root>
