<script lang="ts">
  import PvpFightList from '#lib/components/pvp/PvpFightList.svelte';
  import * as Alert from '#lib/components/ui/alert/index.js';
  import { Button } from '#lib/components/ui/button/index.js';
  import { pvpFightRows } from '#lib/features/pvp/fighter.js';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';

  let { data } = $props();
  const locale = $derived(getLocale());
</script>

<svelte:head>
  <title>{m.pvp_fights_title()} · {m.app_name()}</title>
</svelte:head>

<section class="flex flex-col gap-4" data-testid="pvp-fights">
  <div class="flex flex-col gap-1">
    <h1 class="font-display text-2xl font-bold tracking-tight sm:text-4xl">
      {m.pvp_fights_title()}
    </h1>
    <p class="max-w-prose text-muted-foreground">{m.pvp_fights_lead()}</p>
  </div>
  {#if data.state === 'unavailable'}
    <Alert.Root role="alert">
      <Alert.Title>{m.session_unavailable_title()}</Alert.Title>
      <Alert.Description><p>{m.session_unavailable_text()}</p></Alert.Description>
    </Alert.Root>
  {:else}
    <PvpFightList
      rows={data.fights === null ? null : pvpFightRows(data.fights, locale)}
      nextBefore={data.nextBefore}
      {locale}
    />
  {/if}
  <div>
    <Button variant="link" class="px-0" href={`/${locale}/botarga`}>{m.botarga_title()}</Button>
  </div>
</section>
