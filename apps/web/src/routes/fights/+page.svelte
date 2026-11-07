<script lang="ts">
  import FightList from '#lib/components/replay/FightList.svelte';
  import { fighterDisplay } from '#lib/features/fight/live/fighter-names.js';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';

  let { data } = $props();
  const locale = $derived(getLocale());
</script>

<svelte:head>
  <title>{m.replay_list_title()} · {m.app_name()}</title>
</svelte:head>

<FightList
  rows={data.rows}
  nextBefore={data.nextBefore}
  liveNotice={data.liveNotice}
  display={fighterDisplay}
  {locale}
/>
