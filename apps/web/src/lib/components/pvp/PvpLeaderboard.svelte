<script lang="ts">
  // The PvP leaderboard (021): one season, its eligible accounts by rating, and links to the other
  // seasons. Who does not appear and why is said in general, never by name. Pages are links.
  import type { PvpLeaderboard, SeasonView } from '@torneo/protocol';
  import { Button } from '#lib/components/ui/button/index.js';
  import { formatDate, formatInteger } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';

  interface Props {
    /** Null: the API failed. */
    board: PvpLeaderboard | null;
    /** The season asked for is neither the current one nor a closed one. */
    notFound: boolean;
    seasons: readonly SeasonView[];
    locale: Locale;
  }

  let { board, notFound, seasons, locale }: Props = $props();

  const monthOf = (season: SeasonView) =>
    formatDate(new Date(Number(season.starts_at)), locale, {
      month: 'long',
      year: 'numeric',
      timeZone: 'UTC',
    });
  const others = $derived(seasons.filter((season) => season.id !== board?.season.id));
  const base = $derived(`/${locale}/pvp/leaderboard`);
</script>

<section class="flex flex-col gap-4" data-testid="pvp-leaderboard">
  <div class="flex flex-col gap-1">
    <h1 class="font-display text-2xl font-bold tracking-tight text-balance sm:text-4xl">
      {m.pvp_leaderboard_title()}
    </h1>
    <p class="max-w-prose text-muted-foreground">{m.pvp_leaderboard_lead()}</p>
  </div>

  {#if board === null && notFound}
    <div class="flex flex-col items-start gap-3" data-testid="pvp-leaderboard-not-found">
      <p role="status">{m.error_season_not_found()}</p>
      <Button variant="outline" href={base}>{m.leaderboard_current_link()}</Button>
    </div>
  {:else if board === null}
    <div class="flex flex-col items-start gap-3" data-testid="pvp-leaderboard-error">
      <p role="status">{m.leaderboard_error()}</p>
      <Button variant="outline" href={base} data-sveltekit-reload>{m.leaderboard_retry()}</Button>
    </div>
  {:else}
    <div class="flex flex-col gap-1">
      <h2 class="font-display text-lg font-semibold" data-testid="pvp-leaderboard-season">
        {m.leaderboard_season({ month: monthOf(board.season) })}
        <span class="text-sm font-normal text-muted-foreground">
          · {board.season.closed ? m.leaderboard_season_closed() : m.leaderboard_season_current()}
        </span>
      </h2>
      <p class="max-w-prose text-sm text-muted-foreground">
        {m.pvp_leaderboard_who({ count: formatInteger(board.min_fights, locale) })}
      </p>
    </div>

    {#if board.entries.length === 0}
      <p data-testid="pvp-leaderboard-empty">{m.leaderboard_empty()}</p>
    {:else}
      <ol class="flex flex-col gap-2" aria-label={m.pvp_leaderboard_title()}>
        {#each board.entries as entry (entry.rank)}
          <li
            class="flex min-h-11 items-center gap-3 rounded-lg border border-border bg-card p-3 text-card-foreground"
            data-testid="pvp-leaderboard-row"
          >
            <span class="w-8 shrink-0 text-center font-display text-lg font-bold tabular-nums">
              <span class="sr-only">{m.leaderboard_rank()}</span>
              {entry.rank}
            </span>
            <span class="flex min-w-0 flex-1 flex-col">
              <span class="truncate font-semibold">{entry.display_name}</span>
              <span class="text-sm text-muted-foreground tabular-nums">
                {m.pvp_leaderboard_row({
                  fights: formatInteger(entry.rated_fights, locale),
                  wins: formatInteger(entry.wins, locale),
                  losses: formatInteger(entry.losses, locale),
                })}
              </span>
            </span>
            <span class="shrink-0 text-end font-medium tabular-nums">
              <span class="sr-only">{m.pvp_reward_rating()}</span>
              {formatInteger(entry.rating, locale)}
            </span>
          </li>
        {/each}
      </ol>
    {/if}
  {/if}

  {#if others.length > 0}
    <nav class="flex flex-col gap-2" aria-label={m.leaderboard_seasons_label()}>
      <h2 class="text-sm font-medium">{m.leaderboard_seasons_label()}</h2>
      <ul class="flex flex-wrap gap-2">
        {#each others as season (season.id)}
          <li>
            <Button variant="outline" href={`${base}?season=${season.id}`}>{monthOf(season)}</Button
            >
          </li>
        {/each}
      </ul>
    </nav>
  {/if}
</section>
