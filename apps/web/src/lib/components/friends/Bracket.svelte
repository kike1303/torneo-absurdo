<script lang="ts">
  // A bracket (023): its rounds side by side on a wide screen, one under the other on a phone.
  // What it shows comes already cut by `bracketRounds`: while hidden, only round 1 has names.
  import type { BracketRound } from '#lib/features/friends/bracket.js';
  import { Badge } from '#lib/components/ui/badge/index.js';
  import { Button } from '#lib/components/ui/button/index.js';
  import { formatInteger } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';

  interface Props {
    rounds: readonly BracketRound[];
    locale: Locale;
  }

  let { rounds, locale }: Props = $props();
</script>

<ol class="flex flex-col gap-6 lg:flex-row lg:items-stretch" data-testid="bracket">
  {#each rounds as round (round.round)}
    <li class="flex min-w-0 flex-1 flex-col gap-3" data-testid="bracket-round">
      <h2 class="font-display text-lg font-semibold">
        {round.final
          ? m.tournament_final()
          : m.tournament_round({ round: formatInteger(round.round, locale) })}
      </h2>
      <ul class="flex flex-1 flex-col justify-around gap-3">
        {#each round.matches as cell (cell.slot)}
          <li
            class="flex flex-col gap-2 rounded-lg border border-border bg-card p-3 text-card-foreground"
            data-testid="bracket-match"
            data-shown={cell.a !== null}
          >
            {#if cell.a === null || cell.b === null}
              <span class="text-muted-foreground">{m.tournament_pending()}</span>
            {:else}
              {#each [cell.a, cell.b] as name, i (i)}
                <span
                  class="flex items-center justify-between gap-2"
                  class:font-semibold={cell.advances === name}
                  class:text-muted-foreground={cell.advances !== null && cell.advances !== name}
                >
                  <span class="truncate">{name}</span>
                  {#if cell.advances === name}
                    <Badge variant="secondary" data-testid="bracket-advances">
                      {m.tournament_advances({ name })}
                    </Badge>
                  {/if}
                </span>
              {/each}
            {/if}
            {#if cell.fightId !== null}
              <Button
                variant="outline"
                class="min-h-11"
                href={`/${locale}/friends/fights/${cell.fightId}`}
                data-testid="bracket-watch"
              >
                {m.tournament_watch()}
              </Button>
            {/if}
          </li>
        {/each}
      </ul>
    </li>
  {/each}
</ol>
