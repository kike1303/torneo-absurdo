<script lang="ts">
  // One store per player fight: the caller keys this by id, so another fight starts over. Its own
  // read of the public detail, so what it plays and what it verifies is what the server publishes.
  import { onMount } from 'svelte';
  import PvpReplayView from '#lib/components/pvp/PvpReplayView.svelte';
  import { pvpFighterName } from '#lib/features/pvp/fighter.js';
  import { PvpReplayStore } from '#lib/features/pvp/replay-store.svelte.js';
  import { realDeps } from '#lib/features/replay/engine-loader.js';
  import { verdictAction } from '#lib/features/replay/verify.js';
  import type { Locale } from '#lib/i18n/locale.js';

  interface Props {
    id: number;
    locale: Locale;
    /** Once the result has been seen: the end of the fight or "see the result". */
    onresultseen?: () => void;
  }

  let { id, locale, onresultseen }: Props = $props();

  // svelte-ignore state_referenced_locally
  const store = new PvpReplayStore(id, (input, init) => fetch(input, init), realDeps());
  onMount(() => {
    void store.start();
  });

  $effect(() => {
    if (store.resultSeen) onresultseen?.();
  });

  const names = $derived(
    store.detail === null
      ? null
      : { a: pvpFighterName(store.detail.a, locale), b: pvpFighterName(store.detail.b, locale) },
  );
</script>

<PvpReplayView
  load={store.load}
  detail={store.detail}
  verdict={store.verdict}
  action={store.verdict === null ? 'none' : verdictAction(store.verdict)}
  stageVisible={store.stageVisible}
  playbackError={store.playbackError}
  noAnimation={store.noAnimation}
  resultSeen={store.resultSeen}
  roundClock={store.roundClock}
  {names}
  source={store.source}
  onsnapshot={store.onSnapshot}
  onfailure={store.onStageFailure}
  onshowresult={store.showResult}
  onretryverdict={store.retryVerification}
  onretryload={() => void store.start()}
  {locale}
/>
