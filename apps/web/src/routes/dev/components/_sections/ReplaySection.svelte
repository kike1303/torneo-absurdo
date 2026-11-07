<script lang="ts">
  // The parts of a replay with sample data (011). ReplayView is not here: it would load Pixi and WebGL
  // in the catalog, and it is the page itself.
  import type { FightDetail, FightFinal } from '@torneo/protocol';
  import FightDate from '#lib/components/replay/FightDate.svelte';
  import FightList from '#lib/components/replay/FightList.svelte';
  import ReplayControls from '#lib/components/replay/ReplayControls.svelte';
  import ReplayCopyField from '#lib/components/replay/ReplayCopyField.svelte';
  import ReplayFinalNotice from '#lib/components/replay/ReplayFinalNotice.svelte';
  import ReplayLiveNotice from '#lib/components/replay/ReplayLiveNotice.svelte';
  import ReplayVerdict from '#lib/components/replay/ReplayVerdict.svelte';
  import { fighterDisplay } from '#lib/features/fight/live/fighter-names.js';
  import type { FightRow } from '#lib/features/replay/fight-row.js';
  import { verdictAction, type Verdict } from '#lib/features/replay/verify.js';
  import { getLocale } from '#lib/paraglide/runtime.js';
  import type { FightStageState } from '#lib/components/fight/FightStage.svelte';
  import Section from './Section.svelte';

  const HASH = 'a3f1c2d4e5b60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90';
  const locale = $derived(getLocale());
  const noop = (): void => undefined;

  const rows: FightRow[] = [
    {
      id: 12,
      a: 'fontanero-saltarin',
      b: 'guerrero-erizado',
      date: '1800000000000',
      kind: 'played',
    },
    { id: 11, a: 'striker', b: 'turtle', date: '1799999400000', kind: 'cancelled' },
    { id: 10, a: 'tank', b: 'trickster', date: '1799998800000', kind: 'voided' },
  ];

  const detail = (final: FightFinal): FightDetail => ({
    fight: {
      id: 12,
      state: 'settled',
      a: { slug: 'striker', max_hp: 700 },
      b: { slug: 'tank', max_hp: 860 },
      ruleset_version: 1,
      snapshot_sha256: HASH,
      seed_commit: HASH,
      opens_at: '0',
      closes_at: '0',
      live_at: '0',
      live_started_at: '0',
      time_limit_ms: 86_000,
      max_duration_ms: 90_000,
      playback_delay_ms: 3500,
      final,
    },
    spec_a: '{}',
    spec_b: '{}',
    seed: HASH,
    log: '{}',
  });
  const decided = detail({ kind: 'decided', winner: 'a', reason: 'ko' });
  const voided = detail({ kind: 'voided', reason: 'engine_error' });
  const cancelled = detail({ kind: 'cancelled', reason: 'missed_window' });

  const verdicts: { verdict: Verdict | null; detail: FightDetail | null }[] = [
    { verdict: null, detail: decided },
    { verdict: { status: 'match', scope: 'fight' }, detail: decided },
    { verdict: { status: 'match', scope: 'commit' }, detail: voided },
    { verdict: { status: 'match', scope: 'snapshot' }, detail: cancelled },
    { verdict: { status: 'not_applicable' }, detail: cancelled },
    { verdict: { status: 'mismatch', check: 'log' }, detail: decided },
    { verdict: { status: 'mismatch', check: 'data' }, detail: null },
    { verdict: { status: 'cannot_check', reason: 'engine_unavailable' }, detail: decided },
    { verdict: { status: 'cannot_check', reason: 'engine_failed' }, detail: decided },
    { verdict: { status: 'cannot_check', reason: 'crypto_unavailable' }, detail: decided },
    { verdict: { status: 'cannot_check', reason: 'ruleset_unknown' }, detail: decided },
    { verdict: { status: 'cannot_check', reason: 'unknown_final' }, detail: decided },
  ];
  const finals: FightFinal[] = [
    { kind: 'cancelled', reason: 'missed_window' },
    { kind: 'cancelled', reason: 'fight_unrunnable' },
    { kind: 'voided', reason: 'snapshot_invalid' },
    { kind: 'voided', reason: 'engine_error' },
  ];
  const phases: FightStageState[] = ['loading', 'ready', 'playing', 'paused', 'finished'];
</script>

<Section title="Fight list" id="replay-list">
  <FightList {rows} nextBefore={10} liveNotice={true} display={fighterDisplay} {locale} />
  <FightList rows={[]} nextBefore={null} liveNotice={false} display={fighterDisplay} {locale} />
  <FightList rows={null} nextBefore={null} liveNotice={false} display={fighterDisplay} {locale} />
</Section>

<Section title="Fight date and live notice" id="replay-date">
  <FightDate ms="1800000000000" {locale} />
  <ReplayLiveNotice />
</Section>

<Section title="Replay controls" id="replay-controls">
  {#each phases as phase (phase)}
    <ReplayControls
      {phase}
      ontoggle={noop}
      onback={noop}
      onforward={noop}
      onrestart={noop}
      onend={noop}
    />
  {/each}
</Section>

<Section title="Replay verdict" id="replay-verdict">
  {#each verdicts as entry, index (index)}
    <ReplayVerdict
      verdict={entry.verdict}
      action={entry.verdict === null ? 'none' : verdictAction(entry.verdict)}
      id={12}
      detail={entry.detail}
      onretry={noop}
      {locale}
    />
  {/each}
  <dl>
    <ReplayCopyField label="Empty field" value={null} testid="replay-field-empty" />
  </dl>
</Section>

<Section title="Replay final notice" id="replay-final">
  {#each finals as final (final.reason)}
    <ReplayFinalNotice {final} />
  {/each}
</Section>
