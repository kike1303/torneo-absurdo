<script lang="ts">
  // What the browser could prove of a player fight (021, with the texts of 011): its log is what the
  // engine gives with its own seed and specs. It never says who won, and a code never shows.
  import type { PvpFightDetail } from '@torneo/protocol';
  import { Button } from '#lib/components/ui/button/index.js';
  import * as Card from '#lib/components/ui/card/index.js';
  import type { Verdict, VerdictAction } from '#lib/features/replay/verify.js';
  import { formatInteger } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';
  import ReplayCopyField from '../replay/ReplayCopyField.svelte';

  interface Props {
    /** Null: checking. */
    verdict: Verdict | null;
    action: VerdictAction;
    /** Null when nothing of the response can be shown. */
    detail: PvpFightDetail | null;
    onretry: () => void;
    locale: Locale;
  }

  let { verdict, action, detail, onretry, locale }: Props = $props();

  const text = $derived.by((): string => {
    if (verdict === null) return m.replay_verify_checking();
    switch (verdict.status) {
      case 'match':
        return m.pvp_verify_match();
      case 'mismatch':
        return m.replay_verify_mismatch();
      case 'not_applicable':
        return m.replay_verify_not_applicable();
      case 'cannot_check':
        return verdict.reason === 'ruleset_unknown'
          ? m.replay_verify_cannot_check_ruleset({
              n: formatInteger(detail?.ruleset_version ?? 0, locale),
            })
          : m.replay_verify_cannot_check();
    }
  });
</script>

<Card.Root data-testid="pvp-verdict" data-status={verdict?.status ?? 'checking'}>
  <Card.Content class="flex flex-col gap-3">
    <h2 class="text-sm font-medium text-muted-foreground">{m.replay_verify_title()}</h2>
    <p aria-live="polite" data-testid="pvp-verdict-text">{text}</p>
    {#if action === 'retry'}
      <div>
        <Button variant="outline" onclick={onretry}>{m.replay_verify_retry()}</Button>
      </div>
    {:else if action === 'reload'}
      <div>
        <Button variant="outline" onclick={() => location.reload()}>
          {m.replay_verify_reload()}
        </Button>
      </div>
    {/if}
    {#if verdict !== null && detail !== null}
      <details class="text-sm" data-testid="pvp-verdict-more">
        <summary
          class="flex min-h-11 cursor-pointer items-center rounded-lg text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background focus-visible:outline-none"
        >
          {m.replay_verify_more()}
        </summary>
        <div class="flex flex-col gap-3 pt-1">
          <p class="text-muted-foreground">{m.pvp_verify_more_text()}</p>
          <dl class="flex flex-col gap-3">
            <ReplayCopyField
              label={m.replay_verify_field_seed()}
              value={detail.seed}
              testid="pvp-field-seed"
            />
            <ReplayCopyField
              label={m.pvp_verify_field_spec_a()}
              value={detail.spec_a}
              testid="pvp-field-spec-a"
            />
            <ReplayCopyField
              label={m.pvp_verify_field_spec_b()}
              value={detail.spec_b}
              testid="pvp-field-spec-b"
            />
          </dl>
        </div>
      </details>
    {/if}
  </Card.Content>
</Card.Root>
