<!-- MODIFICADO: close button text from _a11y-messages.ts instead of "Close" (D8). -->
<script lang="ts">
  import { Dialog as DialogPrimitive } from 'bits-ui';
  import { Button } from '#lib/components/ui/button/index.js';
  import { cn, type WithElementRef } from '#lib/utils.js';
  import { a11y } from '../_a11y-messages.js';
  import type { HTMLAttributes } from 'svelte/elements';

  let {
    ref = $bindable(null),
    class: className,
    children,
    showCloseButton = false,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLDivElement>> & {
    showCloseButton?: boolean;
  } = $props();
</script>

<div
  bind:this={ref}
  data-slot="dialog-footer"
  class={cn(
    '-mx-4 -mb-4 flex flex-col-reverse gap-2 rounded-b-xl border-t bg-muted/50 p-4 sm:flex-row sm:justify-end',
    className,
  )}
  {...restProps}
>
  {@render children?.()}
  {#if showCloseButton}
    <DialogPrimitive.Close>
      {#snippet child({ props })}
        <Button variant="outline" {...props}>{a11y.close()}</Button>
      {/snippet}
    </DialogPrimitive.Close>
  {/if}
</div>
