<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import AppHeader from '#lib/components/layout/AppHeader.svelte';
  import { m } from '#lib/paraglide/messages.js';
  import { initTheme } from '#lib/theme/theme.svelte.js';

  let { data, children } = $props();

  onMount(() => initTheme(data.theme));
</script>

<a
  href="#main"
  class="sr-only rounded-md bg-background px-4 py-3 text-foreground outline-none focus:not-sr-only focus:fixed focus:start-2 focus:top-2 focus:z-50 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
>
  {m.skip_to_content()}
</a>
<AppHeader theme={data.theme} />
<main id="main" class="mx-auto max-w-5xl px-4 py-8">
  {@render children()}
</main>
