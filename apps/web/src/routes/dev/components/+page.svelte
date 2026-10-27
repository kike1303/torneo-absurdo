<script lang="ts">
  import ArenaSection from './_sections/ArenaSection.svelte';
  import ButtonSection from './_sections/ButtonSection.svelte';
  import DisplaySection from './_sections/DisplaySection.svelte';
  import FormSection from './_sections/FormSection.svelte';
  import LayoutSection from './_sections/LayoutSection.svelte';
  import OverlaySection from './_sections/OverlaySection.svelte';

  let { data } = $props();
</script>

<svelte:head>
  <title>Component catalog</title>
</svelte:head>

<!-- Catalog of ADR-0009 D5: one theme per view (?theme=dark|light), demo texts exempt from i18n. -->
<div class="flex flex-col gap-8" data-testid="catalog">
  <h1 class="font-display text-3xl font-bold">Component catalog</h1>
  <LayoutSection theme={data.theme} />
  <ButtonSection />
  <FormSection />
  <DisplaySection />
  <OverlaySection />
  <ArenaSection />
</div>
