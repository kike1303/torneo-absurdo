<script lang="ts">
  import UserBadge from '#lib/components/account/UserBadge.svelte';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';
  import type { ThemeChoice } from '#lib/theme/resolve-theme.js';
  import LocaleSwitcher from './LocaleSwitcher.svelte';
  import ThemeSwitcher from './ThemeSwitcher.svelte';

  // 008 RF-D5: "Sign in", or the alias and the balance, or the session error state.
  let {
    theme,
    session = 'none',
    user = null,
    onThemeSave,
    loginHref,
  }: {
    theme: ThemeChoice;
    /** Built by the layout from a validated path, never from the query (no reflection). */
    loginHref: string;
    session?: 'none' | 'valid' | 'unavailable';
    user?: { display_name: string; balance: number } | null;
    onThemeSave?: (choice: ThemeChoice) => Promise<boolean>;
  } = $props();

  const locale = $derived(getLocale());
</script>

<header class="border-b border-border bg-background" aria-label={m.header_label()}>
  <div class="mx-auto flex max-w-5xl flex-wrap items-center justify-between gap-2 px-4 py-2">
    <a
      href={`/${locale}/`}
      aria-label={m.header_home_link()}
      class="inline-flex min-h-11 items-center rounded-md px-1 font-display text-lg font-semibold outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
    >
      {m.app_name()}
    </a>
    <div
      class="ms-auto flex min-w-0 flex-wrap items-center justify-end gap-2"
      data-testid="header-switchers"
    >
      {#if user}
        <UserBadge
          name={user.display_name}
          balance={user.balance}
          {locale}
          href={`/${locale}/account`}
        />
      {:else if session === 'unavailable'}
        <span class="px-2 text-sm text-muted-foreground" data-testid="header-session-unavailable">
          {m.header_session_unavailable()}
        </span>
      {:else}
        <a
          href={loginHref}
          class="inline-flex min-h-11 items-center rounded-md px-3 text-sm font-medium underline-offset-4 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
          data-testid="header-sign-in"
        >
          {m.header_sign_in()}
        </a>
      {/if}
      <LocaleSwitcher signedIn={user !== null} />
      <ThemeSwitcher initial={theme} onSave={onThemeSave} />
    </div>
  </div>
</header>
