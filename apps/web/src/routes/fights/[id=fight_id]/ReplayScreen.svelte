<script lang="ts">
  // One store per fight: the page keys this by id, so going to another fight starts over.
  import { onMount } from 'svelte';
  import ReplayView from '#lib/components/replay/ReplayView.svelte';
  import { fighterDisplay } from '#lib/features/fight/live/fighter-names.js';
  import { realDeps } from '#lib/features/replay/engine-loader.js';
  import { ReplayStore } from '#lib/features/replay/replay-store.svelte.js';
  import { verdictAction } from '#lib/features/replay/verify.js';
  import type { Locale } from '#lib/i18n/locale.js';

  interface Props {
    id: number;
    names: { a: string; b: string };
    locale: Locale;
  }

  let { id, names, locale }: Props = $props();

  // svelte-ignore state_referenced_locally
  const store = new ReplayStore(id, (input, init) => fetch(input, init), realDeps());
  onMount(() => {
    void store.start();
  });
</script>

<ReplayView
  {id}
  load={store.load}
  detail={store.detail}
  verdict={store.verdict}
  action={store.verdict === null ? 'none' : verdictAction(store.verdict)}
  stageVisible={store.stageVisible}
  decided={store.decided}
  playbackError={store.playbackError}
  noAnimation={store.noAnimation}
  resultSeen={store.resultSeen}
  result={store.result}
  roundClock={store.roundClock}
  {names}
  source={store.source}
  onsnapshot={store.onSnapshot}
  onfailure={store.onStageFailure}
  onshowresult={store.showResult}
  onretryverdict={store.retryVerification}
  onretryload={() => void store.start()}
  display={fighterDisplay}
  {locale}
/>
