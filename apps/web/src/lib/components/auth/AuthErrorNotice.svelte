<script lang="ts">
  import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
  import * as Alert from '#lib/components/ui/alert/index.js';
  import type { ErrorTextKey } from '#lib/features/auth/error-text.js';
  import { m } from '#lib/paraglide/messages.js';

  // Receives a key already resolved from a closed list, never a code nor a value from the URL
  // (008 RF-D6), so nothing the request carried can reach the page.
  let {
    key,
    title,
    testid = 'login-error',
  }: { key: ErrorTextKey; title?: string; testid?: string } = $props();
</script>

<Alert.Root variant="destructive" role="alert">
  <CircleAlertIcon aria-hidden="true" />
  {#if title}
    <Alert.Title>{title}</Alert.Title>
  {/if}
  <Alert.Description>
    <p data-testid={testid}>{m[key]()}</p>
  </Alert.Description>
</Alert.Root>
