<script lang="ts">
  import type { Provider } from '@torneo/protocol';
  import { Button } from '#lib/components/ui/button/index.js';
  import { m } from '#lib/paraglide/messages.js';

  // The submit button of one provider's form (008 RF-D5). The form posts straight to /api.
  let { provider, disabled = false }: { provider: Provider; disabled?: boolean } = $props();

  const labels: Record<Provider, () => string> = {
    google: () => m.login_with_google(),
    discord: () => m.login_with_discord(),
    dev: () => m.login_with_dev(),
  };
</script>

<Button
  type="submit"
  size="lg"
  variant={provider === 'dev' ? 'outline' : 'default'}
  class="w-full whitespace-normal"
  {disabled}
  data-testid={`login-provider-${provider}`}
>
  {labels[provider]()}
</Button>
