<!-- MODIFICADO: black overlay → bg-background/80 (RF-A12, no new token); duration from the --duration-base token (reduced motion, D6). -->
<script lang="ts">
  import { Dialog as DialogPrimitive } from 'bits-ui';
  import { cn } from '#lib/utils.js';

  let {
    ref = $bindable(null),
    class: className,
    ...restProps
  }: DialogPrimitive.OverlayProps = $props();
</script>

<DialogPrimitive.Overlay
  bind:ref
  data-slot="dialog-overlay"
  class={cn(
    'fixed inset-0 isolate z-50 bg-background/80 duration-(--duration-base) data-closed:animate-out data-closed:fade-out-0 data-open:animate-in data-open:fade-in-0 supports-backdrop-filter:backdrop-blur-xs',
    className,
  )}
  {...restProps}
/>
