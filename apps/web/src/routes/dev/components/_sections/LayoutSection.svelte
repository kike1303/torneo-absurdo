<script lang="ts">
  import LocaleSwitcher from '#lib/components/layout/LocaleSwitcher.svelte';
  import ThemeSwitcher from '#lib/components/layout/ThemeSwitcher.svelte';
  import type { ThemeChoice } from '#lib/theme/resolve-theme.js';
  import Section from './Section.svelte';

  let { theme }: { theme: ThemeChoice } = $props();
</script>

<Section title="Header selectors" id="layout">
  <p class="text-muted-foreground">The app header (above) uses these two selectors.</p>
  <div class="flex flex-wrap items-center gap-2">
    <LocaleSwitcher />
    <ThemeSwitcher initial={theme} />
  </div>
</Section>
