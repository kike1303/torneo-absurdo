<script lang="ts">
  // A published value, whole and copyable (011 RF16). An attribute cannot be read on a phone.
  import { Button } from '#lib/components/ui/button/index.js';
  import { m } from '#lib/paraglide/messages.js';
  import { copyText, type CopyOutcome } from '../fight/copy-text.js';

  interface Props {
    label: string;
    value: string | null;
    testid: string;
  }

  let { label, value, testid }: Props = $props();

  let outcome = $state<CopyOutcome | null>(null);

  async function copy(text: string): Promise<void> {
    outcome = await copyText(text, navigator.clipboard);
  }
</script>

<div class="flex flex-col gap-1" data-testid={testid}>
  <dt class="text-xs font-medium text-muted-foreground">{label}</dt>
  <dd class="flex flex-col items-start gap-1">
    {#if value === null}
      <span class="text-muted-foreground">{m.replay_verify_field_none()}</span>
    {:else}
      <code class="block font-mono break-all select-all">{value}</code>
      <div class="flex flex-wrap items-center gap-2">
        <Button
          variant="outline"
          onclick={() => void copy(value)}
          aria-label={m.replay_copy_label({ field: label })}
        >
          {m.replay_copy()}
        </Button>
        <span aria-live="polite" class="text-xs text-muted-foreground empty:hidden">
          {#if outcome === 'copied'}
            {m.replay_copied()}
          {:else if outcome === 'manual'}
            {m.replay_copy_manual()}
          {/if}
        </span>
      </div>
    {/if}
  </dd>
</div>
