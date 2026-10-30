<script lang="ts">
  import { formatCoins } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';

  // The alias (data, not a message) and the balance (Intl) of the header, linked to the account
  // (008 RF-D5). At 375 px the alias is truncated; `title` keeps it whole.
  let {
    name,
    balance,
    locale,
    href,
  }: { name: string; balance: number; locale: Locale; href: string } = $props();

  const coins = $derived(m.coins_amount({ amount: formatCoins(balance, locale) }));
</script>

<a
  {href}
  class="inline-flex min-h-11 max-w-full min-w-0 items-center gap-2 rounded-md px-2 text-sm outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
  aria-label={m.header_account_label({ name, balance: coins })}
  data-testid="header-user"
>
  <span class="max-w-32 truncate font-medium sm:max-w-48" title={name}>{name}</span>
  <span class="shrink-0 text-muted-foreground tabular-nums" data-testid="header-balance"
    >{coins}</span
  >
</a>
