<script lang="ts">
  import Leaderboard from '#lib/components/seasons/Leaderboard.svelte';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';

  let { data } = $props();
  const locale = $derived(getLocale());
</script>

<svelte:head>
  <title>{m.leaderboard_title()} · {m.app_name()}</title>
</svelte:head>

<Leaderboard board={data.board} seasons={data.seasons} {locale} />
