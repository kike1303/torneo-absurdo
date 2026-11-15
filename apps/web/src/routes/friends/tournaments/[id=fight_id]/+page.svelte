<script lang="ts">
  // The bracket of a tournament (023). The page carries only who plays and round 1; "show results"
  // reads the whole tournament from the browser, so no outcome is in the HTML (011 RF18, RF20).
  import type { FriendTournament } from '@torneo/protocol';
  import Bracket from '#lib/components/friends/Bracket.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { bracketRounds, parseTournament } from '#lib/features/friends/bracket.js';
  import { formatInteger } from '#lib/i18n/format.js';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';

  let { data } = $props();
  const locale = $derived(getLocale());

  /** The whole tournament, once asked for; it belongs to the id it was read for. */
  let full = $state.raw<FriendTournament | null>(null);
  let reading = $state<'idle' | 'reading' | 'failed'>('idle');
  const results = $derived(full !== null && full.id === data.id ? full : null);

  async function showResults(): Promise<void> {
    const id = data.id;
    reading = 'reading';
    try {
      const response = await fetch(`/api/friends/tournaments/${id}`, {
        headers: { accept: 'application/json' },
        cache: 'no-store',
        signal: AbortSignal.timeout(10_000),
      });
      const tournament =
        response.status === 200 ? parseTournament((await response.json()) as unknown, id) : null;
      if (tournament === null) throw new Error('unavailable');
      full = tournament;
      reading = 'idle';
    } catch {
      reading = 'failed';
    }
  }

  const rounds = $derived(
    data.status !== 'ready'
      ? null
      : results !== null
        ? bracketRounds(results, true)
        : bracketRounds(data.tournament, false),
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
    {#if results !== null}
      <p role="status" class="font-display text-xl font-semibold" data-testid="tournament-champion">
        {m.tournament_champion({ name: results.champion })}
      </p>
    {:else}
      <div class="flex flex-col items-start gap-2">
        <p class="max-w-prose text-sm">{m.tournament_results_hidden()}</p>
        <Button
          class="min-h-11"
          disabled={reading === 'reading'}
          onclick={() => void showResults()}
          data-testid="tournament-show-results"
        >
          {reading === 'failed' ? m.replay_load_retry() : m.tournament_show_results()}
        </Button>
        {#if reading === 'failed'}
          <p role="status" class="text-sm">{m.replay_load_error()}</p>
        {/if}
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
