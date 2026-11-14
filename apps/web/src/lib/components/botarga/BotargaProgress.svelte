<script lang="ts">
  // Where my botarga stands (021 "Web"): level and its XP, rating, the attacks of today and the
  // points not spent yet. Every number is the server's.
  import type { BotargaView } from '@torneo/protocol';
  import { Badge } from '#lib/components/ui/badge/index.js';
  import * as Card from '#lib/components/ui/card/index.js';
  import { formatInteger, formatPercent } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';

  let { botarga, locale }: { botarga: BotargaView; locale: Locale } = $props();

  const progress = $derived(botarga.progress);
  const max = $derived(progress.xp_to_next === 0);
  const share = $derived(max ? 1 : progress.xp / progress.xp_to_next);
</script>

<Card.Root data-testid="botarga-progress">
  <Card.Content class="flex flex-col gap-4">
    <div class="flex flex-wrap items-baseline justify-between gap-2">
      <p class="font-display text-2xl font-bold" data-testid="botarga-level">
        {m.botarga_level({ level: formatInteger(progress.level, locale) })}
      </p>
      <p class="text-sm text-muted-foreground tabular-nums">
        {m.botarga_rating({ rating: formatInteger(botarga.rating, locale) })}
      </p>
    </div>
    <div class="flex flex-col gap-1">
      <div
        class="h-3 w-full overflow-hidden rounded-full bg-muted"
        role="progressbar"
        aria-label={m.botarga_xp_label()}
        aria-valuemin={0}
        aria-valuemax={max ? 1 : progress.xp_to_next}
        aria-valuenow={max ? 1 : progress.xp}
      >
        <div
          class="h-full rounded-full bg-primary"
          style:width={`${Math.round(share * 100)}%`}
        ></div>
      </div>
      <p class="text-sm text-muted-foreground tabular-nums" data-testid="botarga-xp">
        {max
          ? m.botarga_xp_max()
          : m.botarga_xp({
              xp: formatInteger(progress.xp, locale),
              next: formatInteger(progress.xp_to_next, locale),
            })}
      </p>
    </div>
    <div class="flex flex-wrap gap-2 text-sm">
      <Badge variant="outline">
        {m.botarga_daily({ n: formatInteger(progress.daily.attacks, locale) })}
      </Badge>
      {#if progress.daily.factor_pct < 100}
        <Badge variant="secondary" data-testid="botarga-factor">
          {m.botarga_factor({ pct: formatPercent(progress.daily.factor_pct / 100, locale) })}
        </Badge>
      {/if}
      {#if progress.budget_free > 0}
        <Badge data-testid="botarga-budget-free">
          {m.botarga_budget_free({ n: formatInteger(progress.budget_free, locale) })}
        </Badge>
      {/if}
    </div>
  </Card.Content>
</Card.Root>
