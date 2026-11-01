<script lang="ts">
  // The home page of the live fights (010 RF54, plan-d decision 14). Data by props: the state and
  // its decisions live in lib/features/fight/live (ADR-0009 D4).
  import type { RendererErrorCode, RendererSnapshot, Side } from '@torneo/renderer';
  import { Badge } from '#lib/components/ui/badge/index.js';
  import type { FightDebug } from '#lib/features/fight/live/live-store.svelte.js';
  import type { FighterDisplay } from '#lib/features/fight/live/fighter-names.js';
  import type { TextScene } from '#lib/features/fight/live/text-scene.js';
  import type { View } from '#lib/features/fight/live/view-state.js';
  import type { LiveSession } from '#lib/features/fight/live-session.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';
  import FightBillboard from './FightBillboard.svelte';
  import FightNotice from './FightNotice.svelte';
  import FightResultCard from './FightResultCard.svelte';
  import FightStage from './FightStage.svelte';
  import RoundClock from './RoundClock.svelte';

  interface Props {
    view: View;
    serverNow: number;
    /** Ms left of the round; null when it does not show. */
    roundClock: number | null;
    /** The fight without canvas. */
    textScene: TextScene | null;
    degraded: boolean;
    playbackError: boolean;
    noAnimation: boolean;
    stageFight: number | null;
    session: () => Promise<LiveSession>;
    onsnapshot: (snapshot: RendererSnapshot | null) => void;
    onpainted: () => void;
    onfailure: (code: RendererErrorCode) => void;
    display: (slug: string, locale: Locale) => FighterDisplay;
    debug: FightDebug | null;
  }

  let {
    view,
    serverNow,
    roundClock,
    textScene,
    degraded,
    playbackError,
    noAnimation,
    stageFight,
    session,
    onsnapshot,
    onpainted,
    onfailure,
    display,
    debug,
  }: Props = $props();

  const locale = $derived(getLocale());
  const liveFight = $derived(view.live);
  const liveNames = $derived(
    liveFight === null
      ? null
      : {
          a: display(liveFight.a.slug, locale).name,
          b: display(liveFight.b.slug, locale).name,
        },
  );
  const upcoming = $derived(view.upcoming);
  const remaining = $derived(view.target === null ? null : view.target - serverNow);
  const sides: Side[] = ['a', 'b'];
  // The names of the canvas: the fight on the stage is the live one, or the one that comes.
  const stageNames = $derived.by(() => {
    const fight = liveFight ?? upcoming;
    return fight === null
      ? { a: '', b: '' }
      : { a: display(fight.a.slug, locale).name, b: display(fight.b.slug, locale).name };
  });
</script>

<section
  class="relative flex flex-col gap-4 overflow-x-clip"
  data-testid="fight-home"
  data-view={debug === null ? undefined : view.kind}
  data-stage={debug === null ? undefined : view.stage}
  data-t={debug?.t ?? undefined}
  data-t-at={debug?.tAt ?? undefined}
  data-events={debug?.events ?? undefined}
  data-event-digest={debug?.digest ?? undefined}
  data-subscribes={debug?.subscribes ?? undefined}
  data-ws-failures={debug?.failures ?? undefined}
  data-connection={debug?.connection ?? undefined}
  data-painted-t={debug?.paintedT ?? undefined}
>
  <h1 class="font-display text-2xl font-bold tracking-tight text-balance sm:text-4xl">
    {m.home_title()}
  </h1>

  {#if degraded}
    <FightNotice text={m.fight_notice_degraded()} testid="fight-notice-degraded" />
  {/if}
  {#if view.kind === 'halted'}
    <FightNotice text={m.fight_view_halted()} testid="fight-halted" />
  {/if}
  {#if playbackError}
    <FightNotice text={m.fight_notice_playback_error()} testid="fight-notice-playback" />
  {:else if noAnimation}
    <FightNotice text={m.fight_notice_no_animation()} testid="fight-notice-no-animation" />
  {/if}

  <div
    class="flex flex-col gap-4 lg:grid lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)] lg:items-start"
  >
    <div class="relative flex min-w-0 flex-col gap-4">
      {#if view.kind === 'error'}
        <p role="status" data-testid="fight-error">{m.fight_view_error()}</p>
      {:else if view.kind === 'loading'}
        <p role="status" data-testid="fight-loading">{m.fight_view_loading()}</p>
      {:else if view.kind === 'starting'}
        <p data-testid="fight-starting">{m.fight_view_starting()}</p>
      {/if}

      {#if upcoming !== null && (view.kind === 'next' || view.kind === 'about_to_start')}
        <FightBillboard
          fight={upcoming}
          aboutToStart={view.kind === 'about_to_start'}
          remainingMs={remaining}
          {display}
          {locale}
        />
      {/if}

      {#if view.kind === 'live' && liveFight !== null && liveNames !== null}
        <div class="flex flex-wrap items-center gap-2" data-testid="fight-live">
          {#if view.liveBadge}
            <Badge variant="destructive" data-testid="fight-live-badge">{m.fight_view_live()}</Badge
            >
          {/if}
          <p class="font-display font-semibold">
            {liveNames.a}
            {m.fight_view_versus()}
            {liveNames.b}
          </p>
        </div>
        {#if textScene !== null}
          <ul class="flex flex-col gap-1 tabular-nums" data-testid="fight-text-hp">
            {#each sides as side (side)}
              <li>
                {m.fight_text_hp({
                  name: liveNames[side],
                  hp: textScene.hp[side],
                  max: liveFight[side].max_hp,
                })}
              </li>
            {/each}
          </ul>
        {/if}
      {/if}

      {#if view.stage !== 'none' && stageFight !== null}
        <!-- Hidden, it keeps its size: with 0 the canvas has no measure, and its first frame means
             nothing (plan-d decision 14). -->
        <div
          class={view.stage === 'visible'
            ? 'w-full'
            : 'pointer-events-none invisible absolute inset-x-0 top-0 -z-10 w-full'}
          data-testid="fight-stage-box"
          aria-hidden={view.stage === 'visible' ? undefined : 'true'}
        >
          {#key stageFight}
            <FightStage
              fighters={{ a: 'dummy_a', b: 'dummy_b' }}
              displayNames={stageNames}
              live={session}
              showResult={false}
              {onsnapshot}
              {onpainted}
              {onfailure}
            />
          {/key}
        </div>
      {/if}
    </div>

    <div class="flex min-w-0 flex-col gap-4">
      {#if roundClock !== null}
        <RoundClock ms={roundClock} {locale} />
      {/if}
      <div aria-live="polite" class="empty:hidden" data-testid="fight-result-region">
        {#if view.result !== null}
          <FightResultCard
            result={view.result}
            previous={view.kind !== 'live'}
            {display}
            {locale}
          />
        {/if}
      </div>
      {#if view.kind !== 'live'}
        <p class="max-w-prose text-muted-foreground">{m.home_lead()}</p>
      {/if}
    </div>
  </div>
</section>
