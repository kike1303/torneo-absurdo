<script lang="ts">
  // The betting panel of the home page (012 RF41, RF42). Data by props: panelState decides, and this
  // only draws its three zones (the bonus, the result of `last`, the fight) and the bet on the fight
  // in course. It never imports the renderer.
  import type { FightSummary, Side } from '@torneo/protocol';
  import { enhance, type SubmitFunction } from '$app/forms';
  import { Button } from '#lib/components/ui/button/index.js';
  import * as Card from '#lib/components/ui/card/index.js';
  import type { FighterDisplay } from '#lib/features/fight/live/fighter-names.js';
  import type { PanelState, ShownResult } from '#lib/features/bets/panel-state.js';
  import { lostAmount, rakeShare } from './presentation.js';
  import type { QuoteState } from '#lib/features/bets/quote-client.js';
  import { formatCoins, formatDate, formatPercent } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';
  import BetForm from './BetForm.svelte';
  import BetPool from './BetPool.svelte';

  interface Props {
    panel: PanelState;
    /** The result of `last` is of a fight that cannot be verified (012 RF22). */
    unverifiable: boolean;
    display: (slug: string, locale: Locale) => FighterDisplay;
    locale: Locale;
    signInHref: string;
    clientKey: string;
    keptAmount: string | null;
    keptSide: Side | null;
    quote: QuoteState;
    onquote: (fightId: number, side: Side | null, amount: string) => void;
    onrefreshquote: () => void;
    onsettled: (kind: string) => void;
    onreload: () => void;
  }

  let {
    panel,
    unverifiable,
    display,
    locale,
    signInHref,
    clientKey,
    keptAmount,
    keptSide,
    quote,
    onquote,
    onrefreshquote,
    onsettled,
    onreload,
  }: Props = $props();

  const coins = (amount: number) => m.coins_amount({ amount: formatCoins(amount, locale) });
  const namesOf = (fight: FightSummary): Record<Side, string> => ({
    a: display(fight.a.slug, locale).name,
    b: display(fight.b.slug, locale).name,
  });
  const zone = $derived(panel.fight);
  const rake = $derived(
    zone.kind === 'open' || zone.kind === 'signed_out' ? (zone.pool?.rake_bp ?? null) : null,
  );
  let claiming = $state(false);

  const claim: SubmitFunction = () => {
    claiming = true;
    return async ({ result, update }) => {
      await update();
      claiming = false;
      onsettled(result.type);
    };
  };

  function resultText({ result }: ShownResult): string {
    switch (result.outcome) {
      case 'won':
        return result.net === 0
          ? m.bet_result_won_even()
          : m.bet_result_won({ payout: coins(result.payout), net: coins(result.net) });
      case 'lost':
        return m.bet_result_lost({ amount: coins(lostAmount(result.net)) });
      case 'refunded':
        switch (result.refund_reason) {
          case 'draw':
            return m.bet_result_draw({ amount: coins(result.payout) });
          case 'one_sided':
            return m.bet_result_one_sided({ amount: coins(result.payout) });
          case 'cancelled':
            return m.bet_result_cancelled({ amount: coins(result.payout) });
          default:
            return unverifiable
              ? m.bet_result_voided_unverifiable({ amount: coins(result.payout) })
              : m.bet_result_voided({ amount: coins(result.payout) });
        }
    }
  }
</script>

{#if zone.kind !== 'none' || panel.bonus !== null || panel.result !== null || panel.placed !== null}
  <Card.Root data-testid="bet-panel" data-zone={zone.kind}>
    <Card.Content class="flex flex-col gap-4">
      <h2 class="font-display text-lg font-semibold">{m.bet_panel_title()}</h2>

      <div aria-live="polite" class="empty:hidden" data-testid="bet-result-region">
        {#if panel.result !== null}
          <p
            class="font-medium"
            data-testid="bet-result"
            data-outcome={panel.result.result.outcome}
            data-fight={panel.result.fight.id}
          >
            {resultText(panel.result)}
          </p>
        {/if}
      </div>

      {#if panel.bonus !== null}
        <form method="post" action="?/claimBonus" class="flex flex-col gap-2" use:enhance={claim}>
          <p class="text-sm">{m.bet_bonus_lead()}</p>
          <Button type="submit" disabled={claiming} data-testid="bet-bonus-claim">
            {m.bet_bonus_claim({ amount: coins(panel.bonus) })}
          </Button>
        </form>
      {/if}

      {#if panel.placed !== null}
        {@const names = namesOf(panel.placed.fight)}
        {@const side = panel.placed.mine.side}
        <p data-testid="bet-placed" data-halted={panel.placed.halted}>
          {#if side !== null}
            {(panel.placed.halted ? m.bet_mine_halted : m.bet_mine_closed)({
              amount: coins(panel.placed.mine.staked),
              name: names[side],
            })}
          {/if}
        </p>
      {/if}

      {#if zone.kind === 'signed_out'}
        {#if zone.fight !== null && zone.pool !== null}
          <BetPool pool={zone.pool} names={namesOf(zone.fight)} {locale} />
        {/if}
        <Button href={signInHref} data-testid="bet-sign-in">{m.bet_panel_sign_in()}</Button>
      {:else if zone.kind === 'unavailable'}
        {#if zone.fight !== null && zone.pool !== null}
          <BetPool pool={zone.pool} names={namesOf(zone.fight)} {locale} />
        {/if}
        <p role="status" data-testid="bet-unavailable">{m.bet_panel_unavailable()}</p>
      {:else if zone.kind === 'open'}
        {@const names = namesOf(zone.fight)}
        {#if zone.mine.side !== null}
          <p data-testid="bet-mine">
            {m.bet_mine({ amount: coins(zone.mine.staked), name: names[zone.mine.side] })}
          </p>
        {/if}
        {#if zone.wallet.probation !== null}
          <div class="flex flex-col gap-1 text-sm" data-testid="bet-probation">
            <p>{m.bet_probation({ cap: coins(zone.wallet.fight_cap) })}</p>
            <p class="text-muted-foreground">
              {m.bet_probation_progress({
                days: String(zone.wallet.probation.days_left),
                fights: String(zone.wallet.probation.fights_left),
              })}
            </p>
          </div>
        {/if}
        {#key zone.fight.id}
          <BetForm
            open={zone}
            {names}
            {clientKey}
            {keptAmount}
            {keptSide}
            {quote}
            {locale}
            {onquote}
            {onrefreshquote}
            {onsettled}
            {onreload}
          />
        {/key}
      {:else if zone.kind === 'broke'}
        <div class="flex flex-col gap-2" data-testid="bet-broke" data-bailout={zone.bailout}>
          <p class="font-medium">{m.bet_broke_title()}</p>
          {#if zone.placed !== null && zone.placed.mine.side !== null}
            <p data-testid="bet-mine">
              {m.bet_mine({
                amount: coins(zone.placed.mine.staked),
                name: namesOf(zone.placed.fight)[zone.placed.mine.side],
              })}
            </p>
          {/if}
          {#if zone.bailout === 'available'}
            <form method="post" action="?/claimBailout" use:enhance={claim}>
              <Button type="submit" disabled={claiming} data-testid="bet-bailout-claim">
                {m.bet_bailout_claim({ amount: coins(zone.wallet.bailout_amount) })}
              </Button>
            </form>
          {:else if zone.bailout === 'grant_first'}
            <p>{m.bet_broke_grant_first()}</p>
          {:else if zone.bailout === 'not_eligible'}
            <p>{m.bet_broke_not_eligible()}</p>
          {:else}
            <p>
              {m.bet_broke_already_claimed({
                time: formatDate(new Date(Number(zone.wallet.day_resets_at)), locale, {
                  timeStyle: 'short',
                }),
              })}
            </p>
          {/if}
        </div>
      {:else if zone.kind === 'capped'}
        {#if zone.mine.side !== null}
          <p data-testid="bet-mine">
            {m.bet_mine({
              amount: coins(zone.mine.staked),
              name: namesOf(zone.fight)[zone.mine.side],
            })}
          </p>
        {/if}
        <p data-testid="bet-capped">{m.bet_capped()}</p>
      {/if}

      {#if zone.kind !== 'none' && zone.kind !== 'unavailable'}
        <details class="text-sm" data-testid="bet-explainer">
          <summary class="flex min-h-11 cursor-pointer items-center font-medium"
            >{m.bet_explainer_title()}</summary
          >
          <ul class="flex list-disc flex-col gap-1 ps-5">
            <li>{m.bet_explainer_split()}</li>
            {#if rake !== null}
              <li>
                {rake === 0
                  ? m.bet_explainer_no_rake()
                  : m.bet_explainer_rake({ percent: formatPercent(rakeShare(rake), locale) })}
              </li>
            {/if}
            <li>{m.bet_one_sided()}</li>
            <li>{m.bet_final_notice()}</li>
            <li>{m.bet_explainer_fictional()}</li>
          </ul>
        </details>
      {/if}
    </Card.Content>
  </Card.Root>
{/if}
