<script lang="ts">
  // The pool of a fight (012 RF34, RF42): what each side holds and what it pays now. The share of the
  // bar and the multiplier come from presentation.ts, the only file that does arithmetic (RF43).
  import type { FightPool, Side } from '@torneo/protocol';
  import { multiplier, poolShares } from './presentation.js';
  import { formatCoins, formatDecimal } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';

  interface Props {
    pool: FightPool;
    names: Record<Side, string>;
    locale: Locale;
  }

  let { pool, names, locale }: Props = $props();

  const sides: Side[] = ['a', 'b'];
  const shares = $derived(poolShares(pool));
</script>

<div class="flex flex-col gap-2" data-testid="bet-pool">
  <p class="sr-only">{m.bet_pool_label()}</p>
  <div class="flex h-2 w-full overflow-hidden rounded-full bg-muted" aria-hidden="true">
    <div class="bg-primary" style:width="{shares.a * 100}%"></div>
    <div class="bg-muted-foreground" style:width="{shares.b * 100}%"></div>
  </div>
  <ul class="flex flex-col gap-1 text-sm tabular-nums">
    {#each sides as side (side)}
      {@const pays = multiplier(pool[side].payout_bp)}
      <li class="flex flex-wrap justify-between gap-x-2" data-testid="bet-pool-{side}">
        <span>
          {m.bet_pool_side({
            name: names[side],
            amount: m.coins_amount({ amount: formatCoins(pool[side].stake, locale) }),
          })}
        </span>
        <span class="text-muted-foreground">
          {pays === null
            ? m.bet_pool_no_rival()
            : m.bet_pool_pays({ value: formatDecimal(pays, locale, 2) })}
        </span>
      </li>
    {/each}
  </ul>
</div>
