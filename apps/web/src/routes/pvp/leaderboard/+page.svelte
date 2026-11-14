<script lang="ts">
  import PvpLeaderboard from '#lib/components/pvp/PvpLeaderboard.svelte';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';

  let { data } = $props();
  const locale = $derived(getLocale());
</script>

<svelte:head>
  <title>{m.pvp_leaderboard_title()} · {m.app_name()}</title>
</svelte:head>

<PvpLeaderboard board={data.board} notFound={data.notFound} seasons={data.seasons} {locale} />
