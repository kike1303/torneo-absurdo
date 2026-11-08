<script lang="ts">
  // The home page (010 RF53, RF54): the store decides, and the components only draw what it says.
  import { onMount } from 'svelte';
  import type { FightPool, MyFightBets, Side } from '@torneo/protocol';
  import { refreshAll } from '$app/navigation';
  import BetPanel from '#lib/components/bets/BetPanel.svelte';
  import AuthErrorNotice from '#lib/components/auth/AuthErrorNotice.svelte';
  import FightHome from '#lib/components/fight/FightHome.svelte';
  import { resolveApiError } from '#lib/features/auth/error-text.js';
  import { betFailureOf, parseAmount } from '#lib/features/bets/form.js';
  import { panelState, type BetFailure } from '#lib/features/bets/panel-state.js';
  import {
    createQuoteClient,
    type QuoteClient,
    type QuoteInput,
    type QuoteState,
  } from '#lib/features/bets/quote-client.js';
  import { createReread } from '#lib/features/bets/reread.js';
  import { targetFight } from '#lib/features/bets/target.js';
  import { fighterDisplay } from '#lib/features/fight/live/fighter-names.js';
  import { FightStore } from '#lib/features/fight/live/live-store.svelte.js';
  import {
    createVoidStatsReader,
    voidStatsOf,
    type VoidStatsRead,
  } from '#lib/features/fight/live/void-stats.js';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';

  let { data, form } = $props();

  // One per page: `data` only changes by navigating away, or by the rereads of the panel, which do
  // not touch the store: it follows the socket.
  // svelte-ignore state_referenced_locally
  const store = new FightStore(data.board);
  onMount(() => store.connect());

  // 012 RF22: the SSR read the counter if the last fight needed it; one that becomes so by the socket
  // reads it here, once, with its delay.
  // svelte-ignore state_referenced_locally
  const lastId = data.board?.last?.id ?? null;
  // svelte-ignore state_referenced_locally
  const ssrStats = data.voidStats;
  let voidRead = $state.raw<VoidStatsRead | null>(
    ssrStats === null || lastId === null ? null : { fightId: lastId, stats: ssrStats },
  );
  const voidReader = createVoidStatsReader(ssrStats === null ? null : lastId);
  const voidStats = $derived(voidStatsOf(voidRead, store.view.result?.fight.id ?? null));
  $effect(() => {
    const result = store.view.result;
    if (result?.kind !== 'voided_unverifiable') return;
    const fightId = result.fight.id;
    void voidReader.read(fightId).then((stats) => {
      if (stats !== null) voidRead = { fightId, stats };
    });
  });

  // 012 RF41 to RF48: the betting panel.
  const locale = $derived(getLocale());
  const betting = $derived(data.betting);
  const target = $derived(targetFight(store.board));
  const mine = $derived(
    new Map<number, MyFightBets>(Object.values(betting.mine).map((bets) => [bets.fight_id, bets])),
  );
  // plan-c C3: the newest of the SSR read and the socket.
  const pool = $derived.by((): FightPool | null => {
    if (target === null) return null;
    const live = store.pools[target.id] ?? null;
    const read = betting.pool?.fight_id === target.id ? betting.pool : null;
    if (live === null || read === null) return live ?? read;
    return Number(live.server_time) >= Number(read.server_time) ? live : read;
  });
  const failure = $derived.by((): BetFailure | null => {
    const refused = betFailureOf(form);
    if (refused === null || (refused.fight !== null && refused.fight !== target?.id)) return null;
    return refused;
  });
  const claimError = $derived(
    form !== null && form !== undefined && !('amount' in form) && 'code' in form
      ? resolveApiError(form.code)
      : null,
  );
  const panelValue = $derived(
    panelState({
      signedIn: betting.signedIn,
      board: store.board,
      wallet: betting.wallet,
      mine,
      pool,
      shownResult: store.view.result?.fight.id ?? null,
      failure,
    }),
  );

  // RF44, plan-c C2: the key of this read. A refusal keeps it, so a retry is the same bet; a bet
  // placed, or a reload, takes the new one.
  // svelte-ignore state_referenced_locally
  let clientKey = $state(data.betting.clientKey);
  const reread = (): Promise<void> => refreshAll();
  async function onsettled(kind: string): Promise<void> {
    if (kind === 'success') {
      clientKey = data.betting.clientKey;
    } else if (kind === 'failure') {
      await reread();
    }
  }
  async function onreload(): Promise<void> {
    await reread();
    clientKey = data.betting.clientKey;
  }

  // RF36, plan-c C5: the payout by reading again once the view shows the result; and the bets of a
  // window that opened after the page loaded.
  const payoutReader = createReread({
    random: Math.random,
    setTimer: (run, ms) => setTimeout(run, ms),
    reread: () => void reread(),
  });
  const targetReader = createReread({
    random: Math.random,
    setTimer: (run, ms) => setTimeout(run, ms),
    reread: () => void reread(),
  });
  $effect(() => {
    const shown = store.view.result?.fight.id;
    if (shown === undefined) return;
    payoutReader.due(shown, (mine.get(shown)?.staked ?? 0) > 0);
  });
  $effect(() => {
    if (!betting.signedIn || target === null) return;
    targetReader.due(target.id, betting.target !== target.id);
  });

  // RF47, plan-c C4: the estimate.
  let quote = $state.raw<QuoteState>({ status: 'idle' });
  let quoteClient: QuoteClient | null = null;
  let pending: QuoteInput | null = null;
  onMount(() => {
    quoteClient = createQuoteClient({
      fetchFn: (input, init) => fetch(input, init),
      setTimer: (run, ms) => setTimeout(run, ms),
      clearTimer: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
      onChange: (state) => (quote = state),
    });
    quoteClient.request(pending);
    return () => quoteClient?.stop();
  });
  function onquote(fightId: number, side: Side | null, typed: string): void {
    const amount = parseAmount(typed);
    const input =
      side === null || amount === null || amount === 0 ? null : { fightId, side, amount };
    pending = input;
    quoteClient?.request(input);
  }
  let seenPool: FightPool | null = null;
  $effect(() => {
    const current = pool;
    if (seenPool !== null && current !== null && current !== seenPool) {
      quoteClient?.poolChanged(current.fight_id);
    }
    seenPool = current;
  });

  const signInHref = $derived(`/${locale}/login?next=${encodeURIComponent(`/${locale}/`)}`);
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
>
  {#snippet panel()}
    {#if claimError !== null}
      <AuthErrorNotice key={claimError} testid="bet-claim-error" />
    {/if}
    <BetPanel
      panel={panelValue}
      unverifiable={store.view.result?.kind === 'voided_unverifiable'}
      display={fighterDisplay}
      {locale}
      {signInHref}
      {clientKey}
      keptAmount={failure?.amount ?? null}
      keptSide={failure?.side ?? null}
      {quote}
      {onquote}
      onrefreshquote={() => quoteClient?.refresh()}
      {onsettled}
      {onreload}
    />
  {/snippet}
</FightHome>
