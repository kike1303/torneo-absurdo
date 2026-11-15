<script lang="ts">
  // The bracket of a tournament (023). The results wait for "show results": until then only
  // round 1 says who fights, each match with its private replay.
  import Bracket from '#lib/components/friends/Bracket.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { bracketRounds } from '#lib/features/friends/bracket.js';
  import { formatInteger } from '#lib/i18n/format.js';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';

  let { data } = $props();
  const locale = $derived(getLocale());
  let revealed = $state(false);
  const rounds = $derived(
    data.status === 'ready' ? bracketRounds(data.tournament, revealed) : null,
  );
</script>

<svelte:head>
  <title>
    {data.status === 'ready'
      ? `${m.tournament_title({ owner: data.tournament.owner })} · ${m.app_name()}`
      : m.app_name()}
  </title>
</svelte:head>

<section class="flex flex-col gap-4" data-testid="tournament-page" data-status={data.status}>
  {#if data.status === 'unavailable'}
    <div class="flex flex-col items-start gap-3">
      <p role="status">{m.replay_load_error()}</p>
      <Button
        variant="outline"
        href={`/${locale}/friends/tournaments/${data.id}`}
        data-sveltekit-reload
      >
        {m.replay_load_retry()}
      </Button>
    </div>
  {:else}
    {@const tournament = data.tournament}
    <div class="flex flex-col gap-1">
      <h1 class="font-display text-2xl font-bold tracking-tight text-balance sm:text-4xl">
        {m.tournament_title({ owner: tournament.owner })}
      </h1>
      <p class="max-w-prose text-muted-foreground">
        {m.tournament_lead({ size: formatInteger(tournament.size, locale) })}
      </p>
      <p class="text-sm text-muted-foreground">{m.tournament_private_note()}</p>
    </div>
    {#if revealed}
      <p role="status" class="font-display text-xl font-semibold" data-testid="tournament-champion">
        {m.tournament_champion({ name: tournament.champion })}
      </p>
    {:else}
      <div class="flex flex-col items-start gap-2">
        <p class="max-w-prose text-sm">{m.tournament_results_hidden()}</p>
        <Button
          class="min-h-11"
          onclick={() => (revealed = true)}
          data-testid="tournament-show-results"
        >
          {m.tournament_show_results()}
        </Button>
      </div>
    {/if}
    {#if rounds !== null}
      <Bracket {rounds} {locale} />
    {/if}
  {/if}
  <div>
    <Button variant="link" class="px-0" href={`/${locale}/friends`}>{m.friends_title()}</Button>
  </div>
</section>
