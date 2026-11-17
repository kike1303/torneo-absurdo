<script lang="ts">
  // The confirmation of a purchase (024): the item tried on, its price and the balance it leaves.
  // Buying is a form action with the price shown here and the key of this load; the server checks
  // every rule again and answers with a code. Without the coins, the button is not there.
  import { enhance } from '$app/forms';
  import { Button } from '#lib/components/ui/button/index.js';
  import * as Card from '#lib/components/ui/card/index.js';
  import type { ShopEntry } from '#lib/features/shop/shop.js';
  import { formatCoins, formatInteger } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';
  import { labelText } from './shop-text.js';

  interface Props {
    entry: ShopEntry;
    balance: number;
    clientKey: string;
    locale: Locale;
  }

  let { entry, balance, clientKey, locale }: Props = $props();

  let pending = $state(false);
  const short = $derived(entry.price - balance);
</script>

<Card.Root data-testid="shop-confirm" data-state={entry.state}>
  <Card.Content class="flex flex-col gap-3">
    <h2 class="font-display text-lg font-semibold">{labelText(entry.label, locale)}</h2>
    <p class="text-sm tabular-nums">
      {m.shop_price({ amount: formatCoins(entry.price, locale) })}
    </p>
    {#if entry.state === 'owned'}
      <p data-testid="shop-confirm-note">{m.shop_owned_text()}</p>
      <div>
        <Button variant="outline" class="min-h-11" href={`/${locale}/botarga`}>
          {m.shop_to_botarga()}
        </Button>
      </div>
    {:else if entry.state === 'in_draw'}
      <p data-testid="shop-confirm-note">{m.shop_in_draw_text()}</p>
      <div>
        <Button variant="outline" class="min-h-11" href={`/${locale}/botarga`}>
          {m.shop_to_botarga()}
        </Button>
      </div>
    {:else if entry.state === 'level'}
      <p data-testid="shop-confirm-note">
        {m.shop_level_text({ level: formatInteger(entry.level ?? 1, locale) })}
      </p>
    {:else if short > 0}
      <p data-testid="shop-confirm-note">
        {m.shop_short({ amount: formatCoins(short, locale) })}
      </p>
    {:else}
      <p class="text-sm text-muted-foreground tabular-nums">
        {m.shop_after({ amount: formatCoins(balance - entry.price, locale) })}
      </p>
      <form
        method="post"
        action="?/buy"
        use:enhance={() => {
          pending = true;
          return async ({ update }) => {
            await update({ reset: false });
            pending = false;
          };
        }}
      >
        <input type="hidden" name="kind" value={entry.kind} />
        <input type="hidden" name="item_id" value={entry.id} />
        <input type="hidden" name="expected_price" value={entry.price} />
        <input type="hidden" name="client_key" value={clientKey} />
        <Button type="submit" class="min-h-11" disabled={pending} data-testid="shop-buy">
          {m.shop_buy({ amount: formatCoins(entry.price, locale) })}
        </Button>
      </form>
    {/if}
  </Card.Content>
</Card.Root>
