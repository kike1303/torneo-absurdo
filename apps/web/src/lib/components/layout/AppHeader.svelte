<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';
  import type { ThemeChoice } from '#lib/theme/resolve-theme.js';
  import LocaleSwitcher from './LocaleSwitcher.svelte';
  import ThemeSwitcher from './ThemeSwitcher.svelte';

  let { theme }: { theme: ThemeChoice } = $props();
</script>

<header class="border-b border-border bg-background" aria-label={m.header_label()}>
  <div class="mx-auto flex max-w-5xl flex-wrap items-center justify-between gap-2 px-4 py-2">
    <a
      href={`/${getLocale()}/`}
      aria-label={m.header_home_link()}
      class="inline-flex min-h-11 items-center rounded-md px-1 font-display text-lg font-semibold outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
    >
      {m.app_name()}
    </a>
    <div class="ms-auto flex items-center gap-2" data-testid="header-switchers">
      <LocaleSwitcher />
      <ThemeSwitcher initial={theme} />
    </div>
  </div>
</header>
