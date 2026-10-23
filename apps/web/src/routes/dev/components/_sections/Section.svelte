<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Separator } from '#lib/components/ui/separator/index.js';

  let { title, id, children }: { title: string; id: string; children: Snippet } = $props();
</script>

<section
  class="flex flex-col gap-4"
  aria-labelledby={`section-${id}`}
  data-testid={`section-${id}`}
>
  <h2 id={`section-${id}`} class="font-display text-xl font-semibold">{title}</h2>
  <div class="flex flex-col gap-4">
    {@render children()}
  </div>
  <Separator class="mt-4" />
</section>
