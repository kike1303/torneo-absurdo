<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import AppHeader from '#lib/components/layout/AppHeader.svelte';
  import { saveThemePreference } from '#lib/features/account/preferences.js';
  import { getLocale } from '#lib/paraglide/runtime.js';
  import { m } from '#lib/paraglide/messages.js';
  import { initTheme } from '#lib/theme/theme.svelte.js';

  let { data, children } = $props();

  onMount(() => {
    initTheme(data.theme);
    // Hydration signal for e2e (TA-12): set only after mount, never present in the SSR HTML.
    document.documentElement.dataset.hydrated = '';
  });
</script>

<a
  href="#main"
  class="sr-only rounded-md bg-background px-4 py-3 text-foreground outline-none focus:not-sr-only focus:fixed focus:start-2 focus:top-2 focus:z-50 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
>
  {m.skip_to_content()}
</a>
<AppHeader
  theme={data.theme}
  session={data.session}
  user={data.user}
  onThemeSave={data.user ? (choice) => saveThemePreference(getLocale(), choice) : undefined}
/>
<main id="main" class="mx-auto max-w-5xl px-4 py-8">
  {@render children()}
</main>
