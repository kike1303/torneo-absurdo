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

  let { signedIn = false }: { signedIn?: boolean } = $props();

  const current = $derived(getLocale());

  // Never calls Paraglide `setLocale` (plan-a decision 2): a link to the same path and query in
  // the other locale, plus the `locale` cookie, written only on this explicit choice (RF-A4/A5).
  // A full reload re-renders the SSR shell (`<html lang>`, messages) in the new language.
  function choose(locale: Locale): void {
    writeLocaleCookie(locale);
  }

  // With a session (008 RF-D4, plan-d decision 5): a form of the `locale` action, which saves
  // the profile and the cookie and answers 303 to `to`. A real navigation, no race with a reload.
  const formId = (locale: Locale) => `locale-form-${locale}`;
  function submit(locale: Locale): void {
    const form = document.getElementById(formId(locale));
    if (form instanceof HTMLFormElement) form.requestSubmit();
  }
</script>

{#if signedIn}
  {#each locales as locale (locale)}
    <form id={formId(locale)} method="post" action={`/${current}/preferences?/locale`} hidden>
      <input type="hidden" name="locale" value={locale} />
      <input
        type="hidden"
        name="to"
        value={localizedHref(page.url.pathname, page.url.search, locale)}
      />
    </form>
  {/each}
{/if}

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
      {#if signedIn}
        <DropdownMenu.Item
          onSelect={() => submit(locale)}
          lang={locale}
          aria-current={locale === current ? 'true' : undefined}
          data-testid={`locale-option-${locale}`}
        >
          {names[locale]()}
        </DropdownMenu.Item>
      {:else}
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
      {/if}
    {/each}
  </DropdownMenu.Content>
</DropdownMenu.Root>
