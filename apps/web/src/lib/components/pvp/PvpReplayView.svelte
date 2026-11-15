<script lang="ts">
  // The replay of a player fight (021, with the rules of 011 RF18 to RF24), with its data by props:
  // paused at t = 0, the same controls, and the result only once the fight ends or "see the result"
  // is pressed. The store lives in the page (`routes/pvp/PvpReplayScreen.svelte`).
  import type { PvpFightDetail } from '@torneo/protocol';
  import { Button } from '#lib/components/ui/button/index.js';
  import * as Card from '#lib/components/ui/card/index.js';
  import { Skeleton } from '#lib/components/ui/skeleton/index.js';
  import type { AdaptedTimeline } from '#lib/features/fight/timeline-from-fight-log.js';
  import type { ReplayLoad, ReplaySnapshot } from '#lib/features/replay/replay-store.svelte.js';
  import type { Verdict, VerdictAction } from '#lib/features/replay/verify.js';
  import type { StageLooks } from '#lib/features/pvp/fighter.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';
  import FightNotice from '../fight/FightNotice.svelte';
  import FightStage, { type FightStageState } from '../fight/FightStage.svelte';
  import RoundClock from '../fight/RoundClock.svelte';
  import ReplayControls from '../replay/ReplayControls.svelte';
  import PvpVerdict from './PvpVerdict.svelte';

  const JUMP_MS = 10_000;

  interface Props {
    load: ReplayLoad;
    detail: PvpFightDetail | null;
    verdict: Verdict | null;
    action: VerdictAction;
    stageVisible: boolean;
    playbackError: boolean;
    noAnimation: boolean;
    resultSeen: boolean;
    roundClock: number | null;
    /** The names of both sides, once the detail is in. */
    names: Readonly<{ a: string; b: string }> | null;
    /** What each side wore (TA-67): read by the stage once, when it mounts. */
    looks: StageLooks | null;
    source: () => Promise<AdaptedTimeline>;
    onsnapshot: (snapshot: ReplaySnapshot | null) => void;
    onfailure: (code: string) => void;
    onshowresult: () => void;
    onretryverdict: () => void;
    onretryload: () => void;
    locale: Locale;
  }

  let {
    load,
    detail,
    verdict,
    action,
    stageVisible,
    playbackError,
    noAnimation,
    resultSeen,
    roundClock,
    names,
    looks,
    source,
    onsnapshot,
    onfailure,
    onshowresult,
    onretryverdict,
    onretryload,
    locale,
  }: Props = $props();

  let stage = $state<FightStage | null>(null);
  let phase = $state<FightStageState>('loading');

  const resultText = $derived.by((): string | null => {
    if (detail === null || names === null) return null;
    if (detail.winner === null) return m.fight_result_draw();
    const name = names[detail.winner];
    return detail.end_reason === 'ko'
      ? m.fight_result_ko({ name })
      : m.fight_result_decision({ name });
  });
</script>

<div
  class="flex flex-col gap-4 lg:grid lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)] lg:items-start"
  data-testid="pvp-replay"
  data-load={load}
>
  <div class="flex min-w-0 flex-col gap-4">
    {#if load === 'loading'}
      <div class="flex flex-col gap-2">
        <Skeleton class="aspect-video w-full rounded-md" aria-hidden="true" />
        <p role="status" class="text-sm text-muted-foreground">{m.replay_loading()}</p>
      </div>
    {:else if load === 'unavailable'}
      <div class="flex flex-col items-start gap-3" data-testid="pvp-replay-unavailable">
        <p role="status">{m.replay_load_error()}</p>
        <Button variant="outline" onclick={onretryload}>
          {m.replay_load_retry()}
        </Button>
      </div>
    {/if}

    {#if playbackError}
      <FightNotice text={m.replay_notice_playback_error()} testid="replay-notice-playback" />
    {:else if noAnimation}
      <FightNotice text={m.fight_notice_no_animation()} testid="fight-notice-no-animation" />
    {/if}

    {#if stageVisible && names !== null}
      <FightStage
        bind:this={stage}
        bind:phase
        fighters={{ a: 'dummy_a', b: 'dummy_b' }}
        displayNames={names}
        cosmetics={looks?.cosmetics}
        looks={looks?.looks}
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
  </div>

  <div class="flex min-w-0 flex-col gap-4">
    {#if roundClock !== null}
      <RoundClock ms={roundClock} {locale} />
    {/if}
    {#if load === 'ready' && resultText !== null}
      <div aria-live="polite" class="empty:hidden" data-testid="pvp-result-region">
        {#if resultSeen}
          <Card.Root data-testid="pvp-result">
            <Card.Content>
              <p class="font-display text-xl font-semibold">{resultText}</p>
            </Card.Content>
          </Card.Root>
        {/if}
      </div>
      {#if !resultSeen}
        <div>
          <Button variant="outline" onclick={onshowresult} data-testid="pvp-show-result">
            {m.replay_show_result()}
          </Button>
        </div>
      {/if}
    {/if}
    {#if load === 'ready' || load === 'untrusted'}
      <PvpVerdict
        {verdict}
        {action}
        detail={load === 'ready' ? detail : null}
        onretry={onretryverdict}
        {locale}
      />
    {/if}
  </div>
</div>
