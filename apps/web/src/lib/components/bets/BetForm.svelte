<script lang="ts">
  // The open window of the panel (012 RF42, RF44, RF47): side, amount with its minimum and maximum
  // from the server, the estimate for that amount and side, and the bet. It posts `?/bet`, so it
  // also works without JS, and the same `client_key` goes in a retry (RF25 row 3).
  import type { Side } from '@torneo/protocol';
  import { enhance, type SubmitFunction } from '$app/forms';
  import { Button } from '#lib/components/ui/button/index.js';
  import { Input } from '#lib/components/ui/input/index.js';
  import { Label } from '#lib/components/ui/label/index.js';
  import type { OpenFight } from '#lib/features/bets/panel-state.js';
  import type { QuoteState } from '#lib/features/bets/quote-client.js';
  import { formatCoins } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';
  import BetPool from './BetPool.svelte';

  interface Props {
    open: OpenFight;
    names: Record<Side, string>;
    clientKey: string;
    /** What was typed before a refusal, kept (RF42). */
    keptAmount: string | null;
    keptSide: Side | null;
    quote: QuoteState;
    locale: Locale;
    /** The side and the amount as typed changed: the page asks for the estimate (RF47). */
    onquote: (fightId: number, side: Side | null, amount: string) => void;
    onrefreshquote: () => void;
    /** After the server answered: `success`, `failure`, or another kind. */
    onsettled: (kind: string) => void;
    onreload: () => void;
  }

  let {
    open,
    names,
    clientKey,
    keptAmount,
    keptSide,
    quote,
    locale,
    onquote,
    onrefreshquote,
    onsettled,
    onreload,
  }: Props = $props();

  const sides: Side[] = ['a', 'b'];
  const coins = (amount: number) => m.coins_amount({ amount: formatCoins(amount, locale) });
  // svelte-ignore state_referenced_locally
  let amount = $state(keptAmount ?? '');
  // svelte-ignore state_referenced_locally
  let side = $state<Side | null>(open.mine.side ?? keptSide);
  let sending = $state(false);

  const fightId = $derived(open.fight.id);
  const locked = $derived(open.mine.side);
  const maxStake = $derived(open.mine.max_stake);
  const minBet = $derived(open.wallet.min_bet);
  const rejection = $derived(open.rejection);
  // RF26: only the side already bet on.
  $effect(() => {
    if (locked !== null) side = locked;
  });
  // RF47: one request per change of amount or side, never per pool.
  $effect(() => onquote(fightId, side, amount));

  const errorId = $derived(`bet-error-${fightId}`);
  const quoteId = $derived(`bet-quote-${fightId}`);
  const rejectionText = $derived.by((): string | null => {
    if (rejection === null) return null;
    switch (rejection.exit) {
      case 'state':
        return m.error_bet_window_closed();
      case 'minimum':
        return m.bet_rejected_minimum({ amount: coins(minBet) });
      case 'side':
        return locked === null
          ? m.error_bet_side_locked()
          : m.bet_rejected_side({ name: names[locked] });
      case 'maximum':
        return rejection.code === 'bet.fight_cap_exceeded'
          ? m.bet_rejected_cap({ cap: coins(open.wallet.fight_cap), max: coins(maxStake) })
          : m.error_bet_insufficient_balance();
      case 'capped':
        return m.bet_capped();
      case 'reload':
        return m.bet_rejected_reload();
      case 'retry':
        return rejection.code === 'http.rate_limited'
          ? m.error_http_rate_limited()
          : m.bet_rejected_retry();
    }
  });

  const submit: SubmitFunction = () => {
    sending = true;
    return async ({ result, update }) => {
      // The amount stays on a refusal; a bet placed clears it.
      await update({ reset: false, refreshAll: result.type === 'success' });
      if (result.type === 'success') amount = '';
      sending = false;
      onsettled(result.type);
    };
  };
</script>

<form
  method="post"
  action="?/bet"
  class="flex flex-col gap-4"
  data-testid="bet-form"
  use:enhance={submit}
>
  <input type="hidden" name="fight_id" value={fightId} />
  <input type="hidden" name="client_key" value={clientKey} />

  {#if open.pool !== null}
    <BetPool pool={open.pool} {names} {locale} />
  {/if}

  <fieldset
    class="flex flex-col gap-2"
    aria-describedby={rejection?.exit === 'side' ? errorId : undefined}
  >
    <legend class="mb-2 text-sm font-medium">{m.bet_side_label()}</legend>
    <div class="grid grid-cols-2 gap-2">
      {#each sides as option (option)}
        <label
          class="flex min-h-11 cursor-pointer items-center gap-2 rounded-lg border border-input px-3 py-2 has-checked:border-primary has-checked:bg-muted has-disabled:cursor-not-allowed has-disabled:opacity-50"
          data-testid="bet-side-{option}"
        >
          <input
            type="radio"
            name="side"
            value={option}
            class="size-4 accent-primary"
            bind:group={side}
            disabled={locked !== null && locked !== option}
            required
          />
          <span class="min-w-0 break-words">{names[option]}</span>
        </label>
      {/each}
    </div>
  </fieldset>

  <div class="flex flex-col gap-2">
    <Label for="bet-amount-{fightId}">{m.bet_amount_label()}</Label>
    <Input
      id="bet-amount-{fightId}"
      name="amount"
      inputmode="numeric"
      autocomplete="off"
      bind:value={amount}
      aria-invalid={rejection !== null ? 'true' : undefined}
      aria-describedby="{rejection !== null ? errorId : ''} {quoteId}"
      data-testid="bet-amount"
    />
    <p class="text-sm text-muted-foreground">
      {m.bet_amount_help({ min: coins(minBet), max: coins(maxStake) })}
    </p>
    <div class="grid grid-cols-2 gap-2">
      <Button
        variant="outline"
        onclick={() => (amount = String(minBet))}
        data-testid="bet-amount-min">{m.bet_amount_min()}</Button
      >
      <Button
        variant="outline"
        onclick={() => (amount = String(maxStake))}
        data-testid="bet-amount-max">{m.bet_amount_max()}</Button
      >
    </div>
  </div>

  <div
    id={quoteId}
    class="flex flex-col gap-1 text-sm"
    data-testid="bet-quote"
    data-status={quote.status}
  >
    <noscript><p>{m.bet_quote_unavailable()}</p></noscript>
    {#if quote.status === 'loading'}
      <p class="text-muted-foreground">{m.bet_quote_loading()}</p>
    {:else if quote.status === 'ready' || quote.status === 'stale'}
      {@const name = names[quote.input.side]}
      <p>
        {m.bet_quote_win({
          name,
          amount: coins(quote.quote.amount),
          payout: coins(quote.quote.payout),
        })}
      </p>
      <p>{m.bet_quote_lose({ name, amount: coins(quote.quote.amount) })}</p>
      {#if quote.quote.one_sided}
        <p>{m.bet_one_sided()}</p>
      {/if}
      {#if quote.status === 'stale'}
        <div class="flex flex-wrap items-center gap-2">
          <p class="text-muted-foreground">{m.bet_quote_stale()}</p>
          <Button variant="outline" onclick={onrefreshquote} data-testid="bet-quote-refresh"
            >{m.bet_quote_refresh()}</Button
          >
        </div>
      {/if}
    {:else if quote.status === 'below_minimum'}
      <p class="text-muted-foreground">{m.bet_quote_minimum({ amount: coins(minBet) })}</p>
    {:else if quote.status === 'unavailable'}
      <p class="text-muted-foreground">{m.bet_quote_unavailable()}</p>
    {/if}
  </div>

  {#if open.oneSided}
    <p class="text-sm" data-testid="bet-one-sided">{m.bet_one_sided()}</p>
  {/if}

  {#if rejectionText !== null && rejection !== null}
    <div
      id={errorId}
      role="alert"
      class="flex flex-col gap-2 rounded-lg border border-destructive p-3 text-sm"
      data-testid="bet-rejected"
      data-code={rejection.code}
    >
      <p class="font-medium">{m.bet_rejected()}</p>
      <p>{rejectionText}</p>
      {#if rejection.exit === 'reload'}
        <Button variant="outline" onclick={onreload} data-testid="bet-reload"
          >{m.bet_reload()}</Button
        >
      {/if}
    </div>
  {/if}

  <p class="text-sm text-muted-foreground">
    {m.bet_balance({ amount: coins(open.wallet.balance) })}
  </p>
  <p class="text-sm font-medium" data-testid="bet-final-notice">{m.bet_final_notice()}</p>

  <Button type="submit" size="lg" disabled={sending} aria-busy={sending} data-testid="bet-submit">
    {sending ? m.bet_sending() : open.mine.staked > 0 ? m.bet_submit_more() : m.bet_submit()}
  </Button>
</form>
