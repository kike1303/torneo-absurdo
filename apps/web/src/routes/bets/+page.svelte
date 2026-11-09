<script lang="ts">
  import BetHistory from '#lib/components/bets/BetHistory.svelte';
  import * as Alert from '#lib/components/ui/alert/index.js';
  import { fighterDisplay } from '#lib/features/fight/live/fighter-names.js';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';

  let { data } = $props();
  const locale = $derived(getLocale());
</script>

<svelte:head>
  <title>{m.bet_history_title()} · {m.app_name()}</title>
</svelte:head>

{#if data.state === 'unavailable'}
  <Alert.Root role="alert">
    <Alert.Title>{m.session_unavailable_title()}</Alert.Title>
    <Alert.Description><p>{m.session_unavailable_text()}</p></Alert.Description>
  </Alert.Root>
{:else}
  <BetHistory
    rows={data.rows}
    nextBefore={data.nextBefore}
    before={data.before}
    display={fighterDisplay}
    {locale}
  />
{/if}
