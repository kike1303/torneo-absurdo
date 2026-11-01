<script lang="ts">
  // The parts of the live view with sample data (010 plan-d decision 14). FightHome is not here: it
  // would load Pixi and WebGL in the catalog, and it is the home page.
  import type { FightSummary } from '@torneo/protocol';
  import FightBillboard from '#lib/components/fight/FightBillboard.svelte';
  import FightCommit from '#lib/components/fight/FightCommit.svelte';
  import FightNotice from '#lib/components/fight/FightNotice.svelte';
  import FightResultCard from '#lib/components/fight/FightResultCard.svelte';
  import RoundClock from '#lib/components/fight/RoundClock.svelte';
  import { fighterDisplay } from '#lib/features/fight/live/fighter-names.js';
  import { getLocale } from '#lib/paraglide/runtime.js';
  import Section from './Section.svelte';

  const COMMIT = 'a3f1c2d4e5b60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90';
  const fight: FightSummary = {
    id: 1,
    state: 'betting_open',
    a: { slug: 'fontanero-saltarin', max_hp: 860 },
    b: { slug: 'guerrero-erizado', max_hp: 900 },
    ruleset_version: 1,
    snapshot_sha256: COMMIT,
    seed_commit: COMMIT,
    opens_at: '0',
    closes_at: '0',
    live_at: '0',
    live_started_at: null,
    time_limit_ms: 86_000,
    max_duration_ms: 90_000,
    playback_delay_ms: 3500,
    final: null,
  };
  const unknownSlug: FightSummary = {
    ...fight,
    a: { slug: 'striker', max_hp: 860 },
    b: { slug: 'turtle', max_hp: 900 },
    seed_commit: null,
  };
  const locale = $derived(getLocale());
</script>

<Section title="Fight billboard" id="fight-billboard">
  <FightBillboard
    {fight}
    aboutToStart={false}
    remainingMs={272_000}
    display={fighterDisplay}
    {locale}
  />
  <FightBillboard
    fight={unknownSlug}
    aboutToStart={true}
    remainingMs={0}
    display={fighterDisplay}
    {locale}
  />
</Section>

<Section title="Fight commit" id="fight-commit">
  <FightCommit commit={COMMIT} />
</Section>

<Section title="Round clock" id="round-clock">
  <RoundClock ms={86_000} {locale} />
  <RoundClock ms={0} {locale} />
</Section>

<Section title="Fight results" id="fight-results">
  <FightResultCard
    result={{ fight, kind: 'ko', winner: 'a' }}
    previous={false}
    display={fighterDisplay}
    {locale}
  />
  <FightResultCard
    result={{ fight, kind: 'decision', winner: 'b' }}
    previous={true}
    display={fighterDisplay}
    {locale}
  />
  <FightResultCard
    result={{ fight, kind: 'draw', winner: null }}
    previous={true}
    display={fighterDisplay}
    {locale}
  />
  <FightResultCard
    result={{ fight, kind: 'cancelled', winner: null }}
    previous={true}
    display={fighterDisplay}
    {locale}
  />
  <FightResultCard
    result={{ fight, kind: 'voided', winner: null }}
    previous={true}
    display={fighterDisplay}
    {locale}
  />
</Section>

<Section title="Fight notices" id="fight-notices">
  <FightNotice text="Fights are on hold." testid="sample-fight-notice" />
</Section>
