<script lang="ts">
  // My player fights (021 "Web"): as attacker and as defender, the newest first, with what each gave
  // to me. The defenses are what happened while I was away. Pages are links.
  import type { PvpFightSummary } from '@torneo/protocol';
  import type { PvpFightRow } from '#lib/features/pvp/fighter.js';
  import { Badge } from '#lib/components/ui/badge/index.js';
  import { Button } from '#lib/components/ui/button/index.js';
  import { formatCoins, formatInteger, formatSigned } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';
  import FightDate from '../replay/FightDate.svelte';

  interface Props {
    /** Null: the API failed. */
    rows: readonly PvpFightRow[] | null;
    nextBefore: number | null;
    locale: Locale;
  }

  let { rows, nextBefore, locale }: Props = $props();

  /** Won, lost or a draw, from the side of the user. */
  function outcome(fight: PvpFightSummary): string {
    if (fight.winner === null) return m.pvp_outcome_draw();
    const mine = fight.role === 'attacker' ? 'a' : 'b';
    return fight.winner === mine ? m.pvp_outcome_won() : m.pvp_outcome_lost();
  }
</script>

{#if rows === null}
  <div class="flex flex-col items-start gap-3" data-testid="pvp-fights-error">
    <p role="status">{m.pvp_fights_error()}</p>
    <Button variant="outline" href={`/${locale}/pvp/fights`} data-sveltekit-reload>
      {m.leaderboard_retry()}
    </Button>
  </div>
{:else if rows.length === 0}
  <p data-testid="pvp-fights-empty">{m.pvp_fights_empty()}</p>
{:else}
  <ol class="flex flex-col gap-2" aria-label={m.pvp_fights_title()}>
    {#each rows as { fight, opponent } (fight.id)}
      <li
        class="flex flex-col gap-2 rounded-lg border border-border bg-card p-3 text-card-foreground sm:flex-row sm:items-center"
        data-testid="pvp-fight-row"
        data-role={fight.role}
      >
        <div class="flex min-w-0 flex-1 flex-col gap-1">
          <div class="flex flex-wrap items-center gap-2">
            <Badge variant={fight.role === 'defender' ? 'secondary' : 'outline'}>
              {fight.role === 'attacker' ? m.pvp_role_attacker() : m.pvp_role_defender()}
            </Badge>
            <span class="truncate font-semibold">{opponent}</span>
          </div>
          <span class="text-sm text-muted-foreground">
            {outcome(fight)} · <FightDate ms={fight.created_at} {locale} />
          </span>
        </div>
        <div class="flex items-center gap-3 text-sm tabular-nums">
          <span>{m.pvp_reward_rating()} {formatSigned(fight.reward.rating_delta, locale)}</span>
          <span>{m.pvp_reward_xp()} {formatInteger(fight.reward.xp, locale)}</span>
          {#if fight.role === 'attacker'}
            <span>{m.coins_amount({ amount: formatCoins(fight.reward.coins, locale) })}</span>
          {/if}
          <Button
            variant="outline"
            href={`/${locale}/pvp/fights/${fight.id}`}
            data-testid="pvp-fight-watch"
          >
            {m.pvp_fights_watch()}
          </Button>
        </div>
      </li>
    {/each}
  </ol>
  {#if nextBefore !== null}
    <div>
      <Button
        variant="outline"
        href={`/${locale}/pvp/fights?before=${nextBefore}`}
        data-testid="pvp-fights-more"
      >
        {m.pvp_fights_more()}
      </Button>
    </div>
  {/if}
{/if}
