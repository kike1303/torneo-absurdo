<script lang="ts">
  // There is a fight live, as the SSR saw it (011 RF26): it may go stale, and its link stays valid.
  import { Button } from '#lib/components/ui/button/index.js';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';
</script>

<div
  class="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border bg-muted p-3 text-sm text-foreground"
  data-testid="replay-live-notice"
>
  <p>{m.replay_live_notice()}</p>
  <Button variant="outline" href={`/${getLocale()}/`}>{m.replay_live_notice_link()}</Button>
</div>
