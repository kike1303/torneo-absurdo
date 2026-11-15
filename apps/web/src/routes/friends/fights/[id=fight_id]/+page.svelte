<script lang="ts">
  // The title has who fought and never the result (011 RF20).
  import PvpReplayScreen from '../../../pvp/PvpReplayScreen.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { pvpFighterName } from '#lib/features/pvp/fighter.js';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';

  let { data } = $props();
  const locale = $derived(getLocale());
  const crossing = $derived(
    data.status === 'ready'
      ? m.replay_versus({ a: pvpFighterName(data.a, locale), b: pvpFighterName(data.b, locale) })
      : null,
  );
</script>

<svelte:head>
  <title>{crossing === null ? m.app_name() : `${crossing} · ${m.app_name()}`}</title>
</svelte:head>

<section class="flex flex-col gap-4" data-testid="friend-fight-page" data-status={data.status}>
  {#if crossing !== null}
    <h1 class="font-display text-2xl font-bold tracking-tight text-balance sm:text-4xl">
      {crossing}
    </h1>
    <p class="text-sm text-muted-foreground">{m.friends_private_note()}</p>
  {/if}
  {#if data.status === 'unavailable'}
    <div class="flex flex-col items-start gap-3">
      <p role="status">{m.replay_load_error()}</p>
      <Button variant="outline" href={`/${locale}/friends/fights/${data.id}`} data-sveltekit-reload>
        {m.replay_load_retry()}
      </Button>
    </div>
  {:else}
    {#key data.id}
      <PvpReplayScreen id={data.id} kind="friends" {locale} />
    {/key}
  {/if}
  <div>
    <Button variant="link" class="px-0" href={`/${locale}/friends`}>{m.friends_title()}</Button>
  </div>
</section>
