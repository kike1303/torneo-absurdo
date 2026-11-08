<script lang="ts">
  // What the browser could prove, and only that (011 RF12, RF15, RF16). It never says who won. The
  // codes (`check`, `reason`) never show.
  import type { FightDetail } from '@torneo/protocol';
  import { Button } from '#lib/components/ui/button/index.js';
  import * as Card from '#lib/components/ui/card/index.js';
  import type { Verdict, VerdictAction, VerifyCheck } from '#lib/features/replay/verify.js';
  import { formatInteger } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';
  import ReplayCopyField from './ReplayCopyField.svelte';

  interface Props {
    /** Null: checking. */
    verdict: Verdict | null;
    action: VerdictAction;
    id: number;
    /** Null when nothing of the response can be shown: no field of it is listed (RF24). */
    detail: FightDetail | null;
    onretry: () => void;
    locale: Locale;
  }

  let { verdict, action, id, detail, onretry, locale }: Props = $props();

  const MISMATCH_LINE: Record<VerifyCheck, () => string> = {
    data: m.replay_verify_mismatch_state,
    state: m.replay_verify_mismatch_state,
    commit: m.replay_verify_mismatch_commit,
    snapshot: m.replay_verify_mismatch_snapshot,
    log: m.replay_verify_mismatch_log,
    final: m.replay_verify_mismatch_final,
    summary: m.replay_verify_mismatch_summary,
    bounds: m.replay_verify_mismatch_summary,
  };

  const text = $derived.by((): string => {
    if (verdict === null) return m.replay_verify_checking();
    switch (verdict.status) {
      case 'match':
        if (verdict.scope === 'fight') return m.replay_verify_match_fight();
        return verdict.scope === 'commit'
          ? m.replay_verify_match_commit()
          : m.replay_verify_match_snapshot();
      case 'mismatch':
        return m.replay_verify_mismatch();
      case 'not_applicable':
        return m.replay_verify_not_applicable();
      case 'cannot_check':
        if (verdict.reason === 'ruleset_unknown') {
          const version = detail?.fight.ruleset_version ?? 0;
          return m.replay_verify_cannot_check_ruleset({ n: formatInteger(version, locale) });
        }
        return verdict.reason === 'unknown_final'
          ? m.replay_verify_cannot_check_final()
          : m.replay_verify_cannot_check();
    }
  });

  /** Under a `match` that speaks of a seal, out of every `<details>` (RF16). */
  const showsLimit = $derived(
    verdict?.status === 'match' && (verdict.scope === 'fight' || verdict.scope === 'commit'),
  );
  /** A cancelled fight never had a seed: no sentence about seeds, nor its empty fields. */
  const noSeed = $derived(detail?.fight.final?.kind === 'cancelled');
</script>

<Card.Root data-testid="replay-verdict" data-status={verdict?.status ?? 'checking'}>
  <Card.Content class="flex flex-col gap-3">
    <h2 class="text-sm font-medium text-muted-foreground">{m.replay_verify_title()}</h2>
    <div aria-live="polite" class="flex flex-col gap-2" data-testid="replay-verdict-region">
      <p data-testid="replay-verdict-text">{text}</p>
      {#if verdict?.status === 'mismatch'}
        <p class="text-sm" data-testid="replay-verdict-check" data-check={verdict.check}>
          {MISMATCH_LINE[verdict.check]()}
        </p>
      {/if}
      {#if showsLimit}
        <p class="text-sm text-muted-foreground" data-testid="replay-verdict-limit">
          {m.replay_verify_limit()}
        </p>
      {/if}
    </div>
    {#if action === 'retry'}
      <div>
        <Button variant="outline" onclick={onretry} data-testid="replay-verdict-retry">
          {m.replay_verify_retry()}
        </Button>
      </div>
    {:else if action === 'reload'}
      <div>
        <Button
          variant="outline"
          onclick={() => location.reload()}
          data-testid="replay-verdict-reload"
        >
          {m.replay_verify_reload()}
        </Button>
      </div>
    {/if}
    {#if verdict !== null}
      <details class="text-sm" data-testid="replay-verdict-more">
        <summary
          class="flex min-h-11 cursor-pointer items-center rounded-lg text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background focus-visible:outline-none"
        >
          {m.replay_verify_more()}
        </summary>
        <div class="flex flex-col gap-3 pt-1">
          <p class="text-muted-foreground">
            {noSeed ? m.replay_verify_more_text_no_seed() : m.replay_verify_more_text()}
          </p>
          {#if detail !== null}
            <dl class="flex flex-col gap-3">
              {#if !noSeed}
                <ReplayCopyField
                  label={m.replay_verify_field_seed()}
                  value={detail.seed}
                  testid="replay-field-seed"
                />
                <ReplayCopyField
                  label={m.replay_verify_field_commit()}
                  value={detail.fight.seed_commit}
                  testid="replay-field-commit"
                />
              {/if}
              <ReplayCopyField
                label={m.replay_verify_field_snapshot()}
                value={detail.fight.snapshot_sha256}
                testid="replay-field-snapshot"
              />
              <ReplayCopyField
                label={m.replay_verify_field_ruleset()}
                value={String(detail.fight.ruleset_version)}
                testid="replay-field-ruleset"
              />
            </dl>
          {/if}
          <a
            href={`/api/fights/${id}`}
            rel="external"
            class="flex min-h-11 items-center underline underline-offset-4 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            data-testid="replay-verdict-raw"
          >
            {m.replay_verify_raw_link()}
          </a>
        </div>
      </details>
    {/if}
  </Card.Content>
</Card.Root>
