<script lang="ts">
  // My challenges between friends (023), the newest first, private to who fought. The matches of a
  // tournament are reached from its bracket, never listed here. Pages are links.
  import type { FriendFightSummary } from '@torneo/protocol';
  import { Badge } from '#lib/components/ui/badge/index.js';
  import { Button } from '#lib/components/ui/button/index.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';
  import FightDate from '../replay/FightDate.svelte';

  interface Props {
    /** Null: the API failed. */
    fights: readonly FriendFightSummary[] | null;
    nextBefore: number | null;
    locale: Locale;
  }

  let { fights, nextBefore, locale }: Props = $props();

  /** Won, lost or a draw, from the side of the user. */
  function outcome(fight: FriendFightSummary): string {
    if (fight.winner === null) return m.pvp_outcome_draw();
    const mine = fight.role === 'attacker' ? 'a' : 'b';
    return fight.winner === mine ? m.pvp_outcome_won() : m.pvp_outcome_lost();
  }
</script>

<section class="flex flex-col gap-3" aria-labelledby="friend-fights-title">
  <div class="flex flex-col gap-1">
    <h2 id="friend-fights-title" class="font-display text-lg font-semibold">
      {m.friends_fights_title()}
    </h2>
    <p class="text-sm text-muted-foreground">{m.friends_fights_lead()}</p>
  </div>
  {#if fights === null}
    <div class="flex flex-col items-start gap-3" data-testid="friend-fights-error">
      <p role="status">{m.friends_fights_error()}</p>
      <Button variant="outline" href={`/${locale}/friends`} data-sveltekit-reload>
        {m.leaderboard_retry()}
      </Button>
    </div>
  {:else if fights.length === 0}
    <p data-testid="friend-fights-empty">{m.friends_fights_empty()}</p>
  {:else}
    <ol class="flex flex-col gap-2" aria-label={m.friends_fights_title()}>
      {#each fights as fight (fight.id)}
        <li
          class="flex flex-col gap-2 rounded-lg border border-border bg-card p-3 text-card-foreground sm:flex-row sm:items-center"
          data-testid="friend-fight-row"
          data-role={fight.role}
        >
          <div class="flex min-w-0 flex-1 flex-col gap-1">
            <div class="flex flex-wrap items-center gap-2">
              <Badge variant={fight.role === 'defender' ? 'secondary' : 'outline'}>
                {fight.role === 'attacker'
                  ? m.friends_role_challenger()
                  : m.friends_role_challenged()}
              </Badge>
              <span class="truncate font-semibold">{fight.opponent}</span>
            </div>
            <span class="text-sm text-muted-foreground">
              {outcome(fight)} · <FightDate ms={fight.created_at} {locale} />
            </span>
          </div>
          <Button
            variant="outline"
            class="min-h-11"
            href={`/${locale}/friends/fights/${fight.id}`}
            data-testid="friend-fight-watch"
          >
            {m.pvp_fights_watch()}
          </Button>
        </li>
      {/each}
    </ol>
    {#if nextBefore !== null}
      <div>
        <Button
          variant="outline"
          class="min-h-11"
          href={`/${locale}/friends?before=${nextBefore}`}
          data-testid="friend-fights-more"
        >
          {m.friends_fights_more()}
        </Button>
      </div>
    {/if}
  {/if}
</section>
