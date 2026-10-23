<script lang="ts">
  import * as Card from '#lib/components/ui/card/index.js';
  import { m } from '#lib/paraglide/messages.js';
</script>

<svelte:head>
  <title>{m.app_name()}</title>
</svelte:head>

<section class="flex flex-col gap-6">
  <h1 class="font-display text-3xl font-bold tracking-tight text-balance sm:text-4xl">
    {m.home_title()}
  </h1>
  <p class="max-w-prose text-lg text-muted-foreground">{m.home_lead()}</p>
  <Card.Root class="max-w-prose">
    <Card.Content>
      <p>{m.home_coming_soon()}</p>
    </Card.Content>
  </Card.Root>
</section>
