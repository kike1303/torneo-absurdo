<script lang="ts">
  // In UTC, with its zone in sight, in the SSR and the first render, and local from `onMount`: both
  // sides render the same text at first, so there is no hydration mismatch (011 RF17).
  import { onMount } from 'svelte';
  import { formatDateWithZone } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';

  let { ms, locale }: { ms: string; locale: Locale } = $props();

  let local = $state(false);
  onMount(() => {
    local = true;
  });

  const date = $derived(new Date(Number(ms)));
  const text = $derived(formatDateWithZone(date, locale, local ? undefined : 'UTC'));
</script>

<time datetime={date.toISOString()} data-testid="fight-date" data-local={local ? '' : undefined}>
  {text}
</time>
