<script lang="ts">
  // "Set up a tournament" (023): 4 or 8, and that many minus one of my friends who can fight now.
  // The server draws the order and resolves it at once; the action takes me to its bracket.
  import type { FriendEntry } from '@torneo/protocol';
  import { enhance } from '$app/forms';
  import { Button } from '#lib/components/ui/button/index.js';
  import { formatInteger } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';

  interface Props {
    friends: readonly FriendEntry[];
    clientKey: string;
    locale: Locale;
  }

  let { friends, clientKey, locale }: Props = $props();

  const ready = $derived(friends.filter((f) => f.ready));
  const sizes = $derived([4, 8].filter((size) => ready.length >= size - 1));
  let size = $state(4);
  let picked = $state<string[]>([]);
  const need = $derived(size - 1);
  let pending = $state(false);
</script>

<section
  class="flex flex-col gap-3 rounded-lg border border-border bg-card p-4 text-card-foreground"
  aria-labelledby="tournament-new-title"
  data-testid="tournament-new"
>
  <div class="flex flex-col gap-1">
    <h2 id="tournament-new-title" class="font-display text-lg font-semibold">
      {m.tournament_new_title()}
    </h2>
    <p class="text-sm text-muted-foreground">{m.tournament_new_lead()}</p>
  </div>
  {#if sizes.length === 0}
    <p class="text-sm" data-testid="tournament-need-friends">{m.tournament_need_friends()}</p>
  {:else}
    <form
      method="post"
      action="?/tournament"
      class="flex flex-col gap-3"
      use:enhance={() => {
        pending = true;
        return async ({ update }) => {
          await update({ reset: false });
          pending = false;
        };
      }}
    >
      <input type="hidden" name="client_key" value={clientKey} />
      <fieldset class="flex flex-col gap-2">
        <legend class="text-sm font-medium">{m.tournament_size()}</legend>
        <div class="flex flex-wrap gap-2">
          {#each sizes as option (option)}
            <label
              class="flex min-h-11 cursor-pointer items-center gap-2 rounded-md border border-border px-3 has-[:checked]:border-primary"
            >
              <input type="radio" name="size" value={option} bind:group={size} />
              {m.tournament_size_option({ size: formatInteger(option, locale) })}
            </label>
          {/each}
        </div>
      </fieldset>
      <fieldset class="flex flex-col gap-2">
        <legend class="text-sm font-medium">
          {m.tournament_pick({ n: formatInteger(need, locale) })}
        </legend>
        <div class="grid gap-2 sm:grid-cols-2">
          {#each ready as friend (friend.display_name)}
            <label
              class="flex min-h-11 cursor-pointer items-center gap-2 rounded-md border border-border px-3 has-[:checked]:border-primary"
            >
              <input
                type="checkbox"
                name="display_names"
                value={friend.display_name}
                bind:group={picked}
                data-testid="tournament-pick"
              />
              <span class="truncate">{friend.display_name}</span>
            </label>
          {/each}
        </div>
        <p class="text-sm text-muted-foreground" role="status">
          {m.tournament_picked({
            picked: formatInteger(picked.length, locale),
            n: formatInteger(need, locale),
          })}
        </p>
      </fieldset>
      <div>
        <Button
          type="submit"
          class="min-h-11"
          disabled={pending || picked.length !== need}
          data-testid="tournament-create"
        >
          {pending ? m.tournament_creating() : m.tournament_create()}
        </Button>
      </div>
    </form>
  {/if}
</section>
