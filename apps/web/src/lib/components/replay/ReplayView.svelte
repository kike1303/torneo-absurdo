<script lang="ts">
  // A settled fight (011 RF18 to RF24), with its data by props. Starts paused at t = 0. The result
  // waits for `ending`, `finished` or the button, and it stays once seen (RF20).
  import type { FightDetail } from '@torneo/protocol';
  import { Button } from '#lib/components/ui/button/index.js';
  import { Skeleton } from '#lib/components/ui/skeleton/index.js';
  import type { FighterDisplay } from '#lib/features/fight/live/fighter-names.js';
  import type { FightResult } from '#lib/features/fight/live/view-state.js';
  import type { AdaptedTimeline } from '#lib/features/fight/timeline-from-fight-log.js';
  import type { ReplayLoad, ReplaySnapshot } from '#lib/features/replay/replay-store.svelte.js';
  import type { Verdict, VerdictAction } from '#lib/features/replay/verify.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';
  import FightNotice from '../fight/FightNotice.svelte';
  import FightResultCard from '../fight/FightResultCard.svelte';
  import FightStage, { type FightStageState } from '../fight/FightStage.svelte';
  import RoundClock from '../fight/RoundClock.svelte';
  import ReplayControls from './ReplayControls.svelte';
  import ReplayFinalNotice from './ReplayFinalNotice.svelte';
  import ReplayVerdict from './ReplayVerdict.svelte';

  const JUMP_MS = 10_000;

  interface Props {
    id: number;
    load: ReplayLoad;
    detail: FightDetail | null;
    verdict: Verdict | null;
    action: VerdictAction;
    stageVisible: boolean;
    decided: boolean;
    playbackError: boolean;
    noAnimation: boolean;
    resultSeen: boolean;
    result: FightResult | null;
    roundClock: number | null;
    names: Readonly<{ a: string; b: string }>;
    source: () => Promise<AdaptedTimeline>;
    onsnapshot: (snapshot: ReplaySnapshot | null) => void;
    onfailure: (code: string) => void;
    onshowresult: () => void;
    onretryverdict: () => void;
    onretryload: () => void;
    display: (slug: string, locale: Locale) => FighterDisplay;
    locale: Locale;
  }

  let {
    id,
    load,
    detail,
    verdict,
    action,
    stageVisible,
    decided,
    playbackError,
    noAnimation,
    resultSeen,
    result,
    roundClock,
    names,
    source,
    onsnapshot,
    onfailure,
    onshowresult,
    onretryverdict,
    onretryload,
    display,
    locale,
  }: Props = $props();

  let stage = $state<FightStage | null>(null);
  let phase = $state<FightStageState>('loading');

  /**
   * A lost seed shows neither the check nor the seed (012 RF22): its notice says it cannot be
   * verified. Only data that does not hold, or a browser that could not check, still shows it.
   */
  const quietSecretLost = $derived(
    load === 'ready' &&
      detail?.fight.final?.kind === 'voided' &&
      detail.fight.final.reason === 'secret_lost' &&
      (verdict === null || verdict.status === 'not_applicable'),
  );
</script>

<div
  class="flex flex-col gap-4 lg:grid lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)] lg:items-start"
  data-testid="replay-view"
  data-load={load}
>
  <div class="flex min-w-0 flex-col gap-4">
    {#if load === 'loading'}
      <div class="flex flex-col gap-2">
        <Skeleton class="aspect-video w-full rounded-md" aria-hidden="true" />
        <p role="status" class="text-sm text-muted-foreground" data-testid="replay-loading">
          {m.replay_loading()}
        </p>
      </div>
    {:else if load === 'unavailable'}
      <div class="flex flex-col items-start gap-3" data-testid="replay-unavailable">
        <p role="status">{m.replay_load_error()}</p>
        <Button variant="outline" onclick={onretryload} data-testid="replay-load-retry">
          {m.replay_load_retry()}
        </Button>
      </div>
    {/if}

    {#if playbackError}
      <FightNotice text={m.replay_notice_playback_error()} testid="replay-notice-playback" />
    {:else if noAnimation}
      <FightNotice text={m.fight_notice_no_animation()} testid="fight-notice-no-animation" />
    {/if}

    {#if stageVisible}
      <FightStage
        bind:this={stage}
        bind:phase
        fighters={{ a: 'dummy_a', b: 'dummy_b' }}
        displayNames={names}
        {source}
        showResult={false}
        {onsnapshot}
        {onfailure}
      />
      <ReplayControls
        {phase}
        ontoggle={() => stage?.toggle()}
        onback={() => stage?.seekBy(-JUMP_MS)}
        onforward={() => stage?.seekBy(JUMP_MS)}
        onrestart={() => stage?.restart()}
        onend={() => stage?.seekToEnd()}
      />
    {/if}

    {#if load === 'ready' && detail?.fight.final != null && !decided}
      <ReplayFinalNotice final={detail.fight.final} />
    {/if}
  </div>

  <div class="flex min-w-0 flex-col gap-4">
    {#if roundClock !== null}
      <RoundClock ms={roundClock} {locale} />
    {/if}
    {#if decided && result !== null}
      <div aria-live="polite" class="empty:hidden" data-testid="fight-result-region">
        {#if resultSeen}
          <FightResultCard {result} previous={false} {display} {locale} />
        {/if}
      </div>
      {#if !resultSeen}
        <div>
          <Button variant="outline" onclick={onshowresult} data-testid="replay-show-result">
            {m.replay_show_result()}
          </Button>
        </div>
      {/if}
    {/if}
    {#if (load === 'ready' || load === 'untrusted') && !quietSecretLost}
      <ReplayVerdict
        {verdict}
        {action}
        {id}
        detail={load === 'ready' ? detail : null}
        onretry={onretryverdict}
        {locale}
      />
    {/if}
  </div>
</div>
