<script lang="ts">
  // The home page (010 RF53, RF54): the store decides, and the components only draw what it says.
  import { onMount } from 'svelte';
  import FightHome from '#lib/components/fight/FightHome.svelte';
  import { fighterDisplay } from '#lib/features/fight/live/fighter-names.js';
  import { FightStore } from '#lib/features/fight/live/live-store.svelte.js';
  import { createVoidStatsReader } from '#lib/features/fight/live/void-stats.js';
  import { m } from '#lib/paraglide/messages.js';

  let { data } = $props();

  // One per page: `data` only changes by navigating away.
  // svelte-ignore state_referenced_locally
  const store = new FightStore(data.board);
  onMount(() => store.connect());

  // 012 RF22: the SSR read the counter if the last fight needed it; one that becomes so by the socket
  // reads it here, once, with its delay.
  // svelte-ignore state_referenced_locally
  let voidStats = $state(data.voidStats);
  // svelte-ignore state_referenced_locally
  const voidReader = createVoidStatsReader(
    data.voidStats === null ? null : (data.board?.last?.id ?? null),
  );
  $effect(() => {
    const result = store.view.result;
    if (result?.kind !== 'voided_unverifiable') return;
    void voidReader.read(result.fight.id).then((stats) => {
      if (stats !== null) voidStats = stats;
    });
  });
</script>

<svelte:head>
  <title>{m.app_name()}</title>
</svelte:head>

<FightHome
  view={store.view}
  serverNow={store.serverNow}
  roundClock={store.roundClock}
  textScene={store.textScene}
  degraded={store.degraded}
  playbackError={store.playbackError}
  noAnimation={store.noAnimation}
  stageFight={store.stageFight}
  session={store.openSession}
  onsnapshot={store.onSnapshot}
  onpainted={store.onPainted}
  onfailure={store.onStageFailure}
  display={fighterDisplay}
  debug={data.debug ? store.debug : null}
  {voidStats}
/>
