<script lang="ts">
  import PlusIcon from '@lucide/svelte/icons/plus';
  import { Button, type ButtonVariant } from '#lib/components/ui/button/index.js';
  import Section from './Section.svelte';

  const variants: ButtonVariant[] = [
    'default',
    'secondary',
    'outline',
    'ghost',
    'destructive',
    'link',
  ];
</script>

<Section title="Button" id="button">
  {#each variants as variant (variant)}
    <div class="flex flex-wrap items-center gap-3" data-testid={`button-row-${variant}`}>
      <Button {variant} data-testid={`button-${variant}`}>{variant}</Button>
      <Button {variant} disabled>{variant} disabled</Button>
      <Button {variant} aria-invalid="true">{variant} error</Button>
      <Button {variant} size="icon" aria-label={`${variant} icon`}
        ><PlusIcon aria-hidden="true" /></Button
      >
    </div>
  {/each}
  <div class="flex flex-wrap items-center gap-3">
    <Button size="lg">Large</Button>
    <Button href="#button">Link styled as button</Button>
  </div>
</Section>
