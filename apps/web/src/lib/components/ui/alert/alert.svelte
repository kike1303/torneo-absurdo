<!-- shadcn-svelte alert, adapted: only tokens (ADR-0009 D1); the destructive variant keeps
     foreground text on the background and marks itself with the destructive border and icon. -->
<script lang="ts" module>
  import { type VariantProps, tv } from 'tailwind-variants';

  export const alertVariants = tv({
    base: 'relative grid w-full grid-cols-[0_1fr] items-start gap-y-0.5 rounded-lg border px-4 py-3 text-sm has-[>svg]:grid-cols-[calc(var(--spacing)*4)_1fr] has-[>svg]:gap-x-3 [&>svg]:size-4 [&>svg]:translate-y-0.5',
    variants: {
      variant: {
        default: 'border-border bg-card text-card-foreground [&>svg]:text-foreground',
        destructive: 'border-destructive bg-card text-card-foreground [&>svg]:text-destructive',
      },
    },
    defaultVariants: { variant: 'default' },
  });

  export type AlertVariant = VariantProps<typeof alertVariants>['variant'];
</script>

<script lang="ts">
  import type { HTMLAttributes } from 'svelte/elements';
  import { cn, type WithElementRef } from '#lib/utils.js';

  let {
    ref = $bindable(null),
    class: className,
    variant = 'default',
    children,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLDivElement>> & { variant?: AlertVariant } = $props();
</script>

<div
  bind:this={ref}
  data-slot="alert"
  class={cn(alertVariants({ variant }), className)}
  {...restProps}
>
  {@render children?.()}
</div>
