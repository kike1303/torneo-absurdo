<script lang="ts">
  // The clock of the round, like in an arcade (010 RF55): no progress bar, because the server does
  // not publish the duration. A timer without announcements: it changes every second (RF58).
  import { formatClock } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';

  let { ms, locale }: { ms: number; locale: Locale } = $props();
</script>

<p
  role="timer"
  aria-live="off"
  aria-label={m.fight_round_clock_label()}
  class="font-display text-3xl font-bold tabular-nums"
  data-testid="round-clock"
>
  {formatClock(ms, locale)}
</p>
