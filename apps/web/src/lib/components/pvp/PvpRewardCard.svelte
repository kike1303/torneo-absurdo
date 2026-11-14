<script lang="ts">
  // What a fight gave to the user (021 "Web"): their own change of rating, their XP and their coins,
  // and the step of the day when it was not the full one. Every number is the server's.
  import type { PvpReward } from '@torneo/protocol';
  import * as Card from '#lib/components/ui/card/index.js';
  import { formatCoins, formatInteger, formatPercent, formatSigned } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';

  interface Props {
    reward: PvpReward;
    locale: Locale;
  }

  let { reward, locale }: Props = $props();
</script>

<Card.Root data-testid="pvp-reward">
  <Card.Content class="flex flex-col gap-3">
    <h2 class="text-sm font-medium text-muted-foreground">{m.pvp_reward_title()}</h2>
    <dl class="grid grid-cols-3 gap-2 text-center">
      <div class="flex flex-col gap-1">
        <dt class="text-xs text-muted-foreground">{m.pvp_reward_rating()}</dt>
        <dd class="font-display text-xl font-semibold tabular-nums" data-testid="pvp-reward-rating">
          {formatSigned(reward.rating_delta, locale)}
        </dd>
      </div>
      <div class="flex flex-col gap-1">
        <dt class="text-xs text-muted-foreground">{m.pvp_reward_xp()}</dt>
        <dd class="font-display text-xl font-semibold tabular-nums" data-testid="pvp-reward-xp">
          {formatInteger(reward.xp, locale)}
        </dd>
      </div>
      <div class="flex flex-col gap-1">
        <dt class="text-xs text-muted-foreground">{m.pvp_reward_coins()}</dt>
        <dd class="font-display text-xl font-semibold tabular-nums" data-testid="pvp-reward-coins">
          {formatCoins(reward.coins, locale)}
        </dd>
      </div>
    </dl>
    {#if reward.factor_pct < 100}
      <p class="text-sm text-muted-foreground" data-testid="pvp-reward-factor">
        {m.pvp_reward_factor({ pct: formatPercent(reward.factor_pct / 100, locale) })}
      </p>
    {/if}
  </Card.Content>
</Card.Root>
