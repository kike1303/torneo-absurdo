<script lang="ts">
  import { page } from '$app/state';
  import { Button } from '#lib/components/ui/button/index.js';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';

  // Texts are chosen by `page.status`, never by `page.error.message` (RF-A19, plan-a decision 4).
  const notFound = $derived(page.status === 404);
  const title = $derived(notFound ? m.error_404_title() : m.error_500_title());
  const text = $derived(notFound ? m.error_404_text() : m.error_500_text());
  const errorId = $derived(page.error?.errorId);
</script>

<svelte:head>
  <title>{m.error_page_title({ status: page.status })} · {m.app_name()}</title>
</svelte:head>

<section class="flex max-w-prose flex-col gap-4" data-testid="error-page" data-status={page.status}>
  <h1 class="font-display text-3xl font-bold tracking-tight">{title}</h1>
  <p class="text-muted-foreground">{text}</p>
  {#if errorId}
    <p class="text-sm text-muted-foreground" data-testid="error-id">
      {m.error_id_label({ id: errorId })}
    </p>
  {/if}
  <div>
    <Button href={`/${getLocale()}/`}>{m.error_home_link()}</Button>
  </div>
</section>
