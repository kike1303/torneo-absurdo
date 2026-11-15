<script lang="ts">
  // The tournaments I played (023), the newest first. No champion here: the bracket hides the
  // results until asked (011 RF20). Pages are links.
  import type { FriendTournamentSummary } from '@torneo/protocol';
  import { Button } from '#lib/components/ui/button/index.js';
  import { formatInteger } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';
  import FightDate from '../replay/FightDate.svelte';

  interface Props {
    /** Null: the API failed. */
    tournaments: readonly FriendTournamentSummary[] | null;
    nextBefore: number | null;
    locale: Locale;
  }

  let { tournaments, nextBefore, locale }: Props = $props();
</script>

<section class="flex flex-col gap-3" aria-labelledby="tournaments-title">
  <h2 id="tournaments-title" class="font-display text-lg font-semibold">
    {m.tournaments_title()}
  </h2>
  {#if tournaments === null}
    <div class="flex flex-col items-start gap-3" data-testid="tournaments-error">
      <p role="status">{m.tournaments_error()}</p>
      <Button variant="outline" href={`/${locale}/friends`} data-sveltekit-reload>
        {m.leaderboard_retry()}
      </Button>
    </div>
  {:else if tournaments.length === 0}
    <p data-testid="tournaments-empty">{m.tournaments_empty()}</p>
  {:else}
    <ol class="flex flex-col gap-2" aria-label={m.tournaments_title()}>
      {#each tournaments as tournament (tournament.id)}
        <li
          class="flex flex-col gap-2 rounded-lg border border-border bg-card p-3 text-card-foreground sm:flex-row sm:items-center"
          data-testid="tournament-row"
        >
          <div class="flex min-w-0 flex-1 flex-col gap-1">
            <span class="truncate font-semibold">
              {m.tournament_summary({
                owner: tournament.owner,
                size: formatInteger(tournament.size, locale),
              })}
            </span>
            <span class="text-sm text-muted-foreground">
              <FightDate ms={tournament.created_at} {locale} />
            </span>
          </div>
          <Button
            variant="outline"
            class="min-h-11"
            href={`/${locale}/friends/tournaments/${tournament.id}`}
            data-testid="tournament-watch"
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
          href={`/${locale}/friends?tbefore=${nextBefore}`}
          data-testid="tournaments-more"
        >
          {m.tournaments_more()}
        </Button>
      </div>
    {/if}
  {/if}
</section>
