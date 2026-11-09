<script lang="ts">
  // The bet history of the session's user (019): one card per fight, linked to its replay, with the
  // side, the stake and what happened. Every amount is the server's; the outcome is said in words,
  // never by color alone. Pages are links: no JavaScript needed.
  import FightDate from '#lib/components/replay/FightDate.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import type { HistoryRow } from '#lib/features/bets/history.js';
  import type { FighterDisplay } from '#lib/features/fight/live/fighter-names.js';
  import { formatCoins } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';
  import { lostAmount } from './presentation.js';

  interface Props {
    /** Null: the API failed. */
    rows: readonly HistoryRow[] | null;
    nextBefore: number | null;
    /** The page shown, to retry that same one. */
    before: number | null;
    display: (slug: string, locale: Locale) => FighterDisplay;
    locale: Locale;
  }

  let { rows, nextBefore, before, display, locale }: Props = $props();

  const coins = (amount: number) => m.coins_amount({ amount: formatCoins(amount, locale) });

  function outcomeText(row: HistoryRow): string {
    switch (row.status) {
      case 'pending':
        return m.bet_history_pending();
      case 'won':
        return row.net === 0
          ? m.bet_history_won_even()
          : m.bet_history_won({ net: coins(row.net), payout: coins(row.payout) });
      case 'lost':
        return m.bet_history_lost({ amount: coins(lostAmount(row.net)) });
      case 'refunded': {
        const amount = coins(row.payout);
        switch (row.refundReason) {
          case 'draw':
            return m.bet_history_refund_draw({ amount });
          case 'one_sided':
            return m.bet_history_refund_one_sided({ amount });
          case 'voided':
            return m.bet_history_refund_voided({ amount });
          case 'cancelled':
            return m.bet_history_refund_cancelled({ amount });
          default:
            return m.bet_history_refund({ amount });
        }
      }
    }
  }
</script>

<section class="flex flex-col gap-4" data-testid="bet-history">
  <div class="flex flex-col gap-1">
    <h1 class="font-display text-2xl font-bold tracking-tight text-balance sm:text-4xl">
      {m.bet_history_title()}
    </h1>
    <p class="max-w-prose text-muted-foreground">{m.bet_history_lead()}</p>
  </div>

  {#if rows === null}
    <div class="flex flex-col items-start gap-3" data-testid="bet-history-error">
      <p role="status">{m.bet_history_error()}</p>
      <Button
        variant="outline"
        href={before === null ? `/${locale}/bets` : `/${locale}/bets?before=${before}`}
        data-sveltekit-reload
      >
        {m.bet_history_retry()}
      </Button>
    </div>
  {:else if rows.length === 0}
    <div class="flex flex-col items-start gap-3" data-testid="bet-history-empty">
      <p>{m.bet_history_empty()}</p>
      <Button variant="outline" href={`/${locale}/`}>{m.bet_history_empty_link()}</Button>
    </div>
  {:else}
    <ul class="flex flex-col gap-2">
      {#each rows as row (row.id)}
        {@const names = { a: display(row.a, locale).name, b: display(row.b, locale).name }}
        <li>
          <a
            href={`/${locale}/fights/${row.id}`}
            class="flex min-h-11 flex-col gap-1 rounded-lg border border-border bg-card p-3 text-card-foreground transition-colors hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background focus-visible:outline-none sm:flex-row sm:items-center sm:justify-between sm:gap-4"
            data-testid="bet-history-row"
            data-fight={row.id}
            data-status={row.status}
          >
            <span class="flex flex-col gap-1">
              <span class="font-display font-semibold">
                {m.replay_versus({ a: names.a, b: names.b })}
              </span>
              <span class="text-sm text-muted-foreground tabular-nums">
                {m.bet_history_stake({ amount: coins(row.staked), name: names[row.side] })}
              </span>
            </span>
            <span class="flex flex-col gap-1 text-sm sm:items-end sm:text-right">
              <span
                class={[
                  'font-medium tabular-nums',
                  row.status === 'lost' && 'text-destructive',
                  row.status === 'pending' && 'text-muted-foreground',
                ]}
                data-testid="bet-history-outcome"
              >
                {outcomeText(row)}
              </span>
              <span class="text-muted-foreground"><FightDate ms={row.placedAt} {locale} /></span>
            </span>
          </a>
        </li>
      {/each}
    </ul>
    {#if nextBefore !== null}
      <div>
        <Button
          variant="outline"
          href={`/${locale}/bets?before=${nextBefore}`}
          data-testid="bet-history-more"
        >
          {m.bet_history_more()}
        </Button>
      </div>
    {/if}
  {/if}
</section>
