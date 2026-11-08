<script lang="ts">
  // The past fights (011 RF17): who and when, and how it ended only as held, cancelled or voided, so
  // it does not spoil the fight of whoever wants to watch it. Pages are links: no JavaScript needed.
  import { Button } from '#lib/components/ui/button/index.js';
  import type { FighterDisplay } from '#lib/features/fight/live/fighter-names.js';
  import type { FightRow, FightRowKind } from '#lib/features/replay/fight-row.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';
  import FightDate from './FightDate.svelte';
  import ReplayLiveNotice from './ReplayLiveNotice.svelte';

  interface Props {
    /** Null: the API failed. */
    rows: readonly FightRow[] | null;
    nextBefore: number | null;
    liveNotice: boolean;
    display: (slug: string, locale: Locale) => FighterDisplay;
    locale: Locale;
  }

  let { rows, nextBefore, liveNotice, display, locale }: Props = $props();

  const KIND_TEXT: Record<FightRowKind, () => string> = {
    played: m.replay_list_kind_played,
    cancelled: m.replay_list_kind_cancelled,
    voided: m.replay_list_kind_voided,
  };
</script>

<section class="flex flex-col gap-4" data-testid="fight-list">
  <div class="flex flex-col gap-1">
    <h1 class="font-display text-2xl font-bold tracking-tight text-balance sm:text-4xl">
      {m.replay_list_title()}
    </h1>
    <p class="max-w-prose text-muted-foreground">{m.replay_list_lead()}</p>
  </div>

  {#if liveNotice}
    <ReplayLiveNotice />
  {/if}

  {#if rows === null}
    <div class="flex flex-col items-start gap-3" data-testid="fight-list-error">
      <p role="status">{m.replay_list_error()}</p>
      <Button variant="outline" href={`/${locale}/fights`} data-sveltekit-reload>
        {m.replay_list_retry()}
      </Button>
    </div>
  {:else if rows.length === 0}
    <div class="flex flex-col items-start gap-3" data-testid="fight-list-empty">
      <p>{m.replay_list_empty()}</p>
      <Button variant="outline" href={`/${locale}/`}>{m.replay_list_empty_link()}</Button>
    </div>
  {:else}
    <ul class="flex flex-col gap-2">
      {#each rows as row (row.id)}
        <li>
          <a
            href={`/${locale}/fights/${row.id}`}
            class="flex min-h-11 flex-col gap-1 rounded-lg border border-border bg-card p-3 text-card-foreground transition-colors hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background focus-visible:outline-none sm:flex-row sm:items-center sm:justify-between sm:gap-4"
            data-testid="fight-list-row"
            data-fight={row.id}
          >
            <span class="font-display font-semibold">
              {m.replay_versus({
                a: display(row.a, locale).name,
                b: display(row.b, locale).name,
              })}
            </span>
            <span
              class="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm text-muted-foreground sm:justify-end"
            >
              {#if row.kind !== null}
                <span data-testid="fight-list-kind">{KIND_TEXT[row.kind]()}</span>
              {/if}
              <FightDate ms={row.date} {locale} />
            </span>
          </a>
        </li>
      {/each}
    </ul>
    {#if nextBefore !== null}
      <div>
        <Button
          variant="outline"
          href={`/${locale}/fights?before=${nextBefore}`}
          data-testid="fight-list-more"
        >
          {m.replay_list_more()}
        </Button>
      </div>
    {/if}
  {/if}
</section>
