<script lang="ts">
  // "Search fight" (021): one form with the key of this load. The server resolves the fight at once
  // against the botarga of another user, connected or not; a retry with the same key is the same
  // fight. While it waits, the button says so. A fight does not reload the page data: the new level,
  // XP and draw would tell who won before the replay does (011 RF20), so the page reloads them once
  // the result is seen.
  import { enhance } from '$app/forms';
  import { Button } from '#lib/components/ui/button/index.js';
  import { m } from '#lib/paraglide/messages.js';

  let { clientKey }: { clientKey: string } = $props();
  let pending = $state(false);
</script>

<form
  method="post"
  action="?/attack"
  class="flex flex-col items-start gap-2"
  use:enhance={() => {
    pending = true;
    return async ({ update }) => {
      await update({ reset: false, invalidateAll: false });
      pending = false;
    };
  }}
>
  <input type="hidden" name="client_key" value={clientKey} />
  <Button type="submit" size="lg" class="min-h-11" disabled={pending} data-testid="pvp-search">
    {pending ? m.pvp_searching() : m.pvp_search()}
  </Button>
  <p class="text-sm text-muted-foreground">{m.pvp_search_lead()}</p>
</form>
