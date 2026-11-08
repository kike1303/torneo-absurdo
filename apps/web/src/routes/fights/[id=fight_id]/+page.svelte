<script lang="ts">
  // The title has who fights and never the result (011 RF20).
  import FightDate from '#lib/components/replay/FightDate.svelte';
  import ReplayLiveNotice from '#lib/components/replay/ReplayLiveNotice.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { fighterDisplay } from '#lib/features/fight/live/fighter-names.js';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';
  import ReplayScreen from './ReplayScreen.svelte';

  let { data } = $props();

  const locale = $derived(getLocale());
  const names = $derived(
    data.status === 'unavailable'
      ? null
      : {
          a: fighterDisplay(data.a, locale).name,
          b: fighterDisplay(data.b, locale).name,
        },
  );
  const crossing = $derived(names === null ? null : m.replay_versus(names));
</script>

<svelte:head>
  <title>{crossing === null ? m.app_name() : `${crossing} · ${m.app_name()}`}</title>
</svelte:head>

<section class="flex flex-col gap-4" data-testid="fight-page" data-status={data.status}>
  <div class="flex flex-col gap-1">
    {#if crossing !== null}
      <h1 class="font-display text-2xl font-bold tracking-tight text-balance sm:text-4xl">
        {crossing}
      </h1>
    {/if}
    {#if data.status === 'settled'}
      <p class="text-sm text-muted-foreground"><FightDate ms={data.date} {locale} /></p>
    {/if}
  </div>

  {#if data.liveNotice}
    <ReplayLiveNotice />
  {/if}

  {#if data.status === 'pending'}
    <div class="flex flex-col items-start gap-3" data-testid="fight-pending">
      <p>{m.replay_pending_text()}</p>
      <Button variant="outline" href={`/${locale}/`}>{m.replay_pending_link()}</Button>
    </div>
  {:else if data.status === 'unavailable'}
    <div class="flex flex-col items-start gap-3" data-testid="fight-unavailable">
      <p role="status">{m.replay_load_error()}</p>
      <Button
        variant="outline"
        href={`/${locale}/fights/${data.id}`}
        data-sveltekit-reload
        data-testid="fight-unavailable-retry"
      >
        {m.replay_load_retry()}
      </Button>
    </div>
  {:else if names !== null}
    {#key data.id}
      <ReplayScreen id={data.id} {names} {locale} />
    {/key}
  {/if}

  <div>
    <Button variant="link" class="px-0" href={`/${locale}/fights`} data-testid="fight-back-to-list">
      {m.replay_back_to_list()}
    </Button>
  </div>
</section>
