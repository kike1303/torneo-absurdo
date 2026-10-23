<script lang="ts">
  import LanguagesIcon from '@lucide/svelte/icons/languages';
  import { page } from '$app/state';
  import { Button } from '#lib/components/ui/button/index.js';
  import * as DropdownMenu from '#lib/components/ui/dropdown-menu/index.js';
  import { localizedHref, writeLocaleCookie, type Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale, locales } from '#lib/paraglide/runtime.js';

  // Language names are written in their own language, so they stay the same in every locale.
  const names: Record<Locale, () => string> = {
    en: () => m.locale_name_en(),
    es: () => m.locale_name_es(),
  };

  const current = $derived(getLocale());

  // Never calls Paraglide `setLocale` (plan-a decision 2): a link to the same path and query in
  // the other locale, plus the `locale` cookie, written only on this explicit choice (RF-A4/A5).
  // A full reload re-renders the SSR shell (`<html lang>`, messages) in the new language.
  function choose(locale: Locale): void {
    writeLocaleCookie(locale);
  }
</script>

<DropdownMenu.Root>
  <DropdownMenu.Trigger>
    {#snippet child({ props })}
      <Button
        {...props}
        variant="outline"
        aria-label={m.locale_switcher_current({ name: names[current]() })}
        data-testid="locale-switcher"
      >
        <LanguagesIcon aria-hidden="true" />
        <span lang={current}>{names[current]()}</span>
      </Button>
    {/snippet}
  </DropdownMenu.Trigger>
  <DropdownMenu.Content align="end" class="min-w-44">
    <DropdownMenu.Label>{m.locale_switcher_label()}</DropdownMenu.Label>
    {#each locales as locale (locale)}
      <DropdownMenu.Item>
        {#snippet child({ props })}
          <a
            {...props}
            href={localizedHref(page.url.pathname, page.url.search, locale)}
            hreflang={locale}
            lang={locale}
            aria-current={locale === current ? 'true' : undefined}
            data-sveltekit-reload
            data-testid={`locale-option-${locale}`}
            onclick={() => choose(locale)}
          >
            {names[locale]()}
          </a>
        {/snippet}
      </DropdownMenu.Item>
    {/each}
  </DropdownMenu.Content>
</DropdownMenu.Root>
