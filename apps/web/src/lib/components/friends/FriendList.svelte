<script lang="ts">
  // My friends (023), by name: challenge one (a fight resolved at once, with the key of this load)
  // or remove one, after saying what removing does. A friend who cannot fight now says only that,
  // never why.
  import type { FriendEntry } from '@torneo/protocol';
  import { enhance } from '$app/forms';
  import { Badge } from '#lib/components/ui/badge/index.js';
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

  /** The friend being challenged, while the server resolves the fight. */
  let pending = $state<string | null>(null);
  /** The friend whose removal waits for a confirmation. */
  let confirming = $state<string | null>(null);
</script>

<section class="flex flex-col gap-3" aria-labelledby="friends-list-title">
  <h2 id="friends-list-title" class="font-display text-lg font-semibold">
    {m.friends_list_title()}
  </h2>
  {#if friends.length === 0}
    <p data-testid="friends-empty">{m.friends_empty()}</p>
  {:else}
    <ul class="flex flex-col gap-2" data-testid="friends-list">
      {#each friends as friend (friend.display_name)}
        <li
          class="flex flex-col gap-3 rounded-lg border border-border bg-card p-3 text-card-foreground sm:flex-row sm:items-center"
          data-testid="friend-row"
          data-name={friend.display_name}
          data-ready={friend.ready}
        >
          <div class="flex min-w-0 flex-1 flex-col gap-1">
            <span class="truncate font-semibold">{friend.display_name}</span>
            <span class="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
              {m.botarga_level({ level: formatInteger(friend.level, locale) })}
              {#if !friend.ready}
                <Badge variant="secondary">{m.friends_not_ready()}</Badge>
              {/if}
            </span>
          </div>
          <div class="flex flex-wrap gap-2">
            <form
              method="post"
              action="?/challenge"
              use:enhance={() => {
                pending = friend.display_name;
                return async ({ update }) => {
                  await update({ reset: false, invalidateAll: false });
                  pending = null;
                };
              }}
            >
              <input type="hidden" name="display_name" value={friend.display_name} />
              <input type="hidden" name="client_key" value={clientKey} />
              <Button
                type="submit"
                class="min-h-11"
                disabled={!friend.ready || pending !== null}
                data-testid="friend-challenge"
              >
                {pending === friend.display_name ? m.friends_challenging() : m.friends_challenge()}
              </Button>
            </form>
            {#if confirming !== friend.display_name}
              <Button
                type="button"
                variant="outline"
                class="min-h-11"
                onclick={() => (confirming = friend.display_name)}
                data-testid="friend-remove"
              >
                {m.friends_remove()}
              </Button>
            {/if}
          </div>
          {#if confirming === friend.display_name}
            <form
              method="post"
              action="?/remove"
              class="flex flex-col gap-2 sm:basis-full"
              use:enhance={() => {
                return async ({ update }) => {
                  await update();
                  confirming = null;
                };
              }}
            >
              <p class="text-sm" role="status">
                {m.friends_remove_confirm({ name: friend.display_name })}
              </p>
              <input type="hidden" name="display_name" value={friend.display_name} />
              <div class="flex flex-wrap gap-2">
                <Button
                  type="submit"
                  variant="destructive"
                  class="min-h-11"
                  data-testid="friend-remove-yes"
                >
                  {m.friends_remove_yes()}
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  class="min-h-11"
                  onclick={() => (confirming = null)}
                >
                  {m.friends_remove_cancel()}
                </Button>
              </div>
            </form>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>
