<script lang="ts">
  import DisplayNameForm from '#lib/components/account/DisplayNameForm.svelte';
  import * as Alert from '#lib/components/ui/alert/index.js';
  import { Button } from '#lib/components/ui/button/index.js';
  import { resolveApiError } from '#lib/features/auth/error-text.js';
  import { m } from '#lib/paraglide/messages.js';

  let { data, form } = $props();
</script>

<svelte:head>
  <title>{m.onboarding_title()} · {m.app_name()}</title>
</svelte:head>

<section class="mx-auto flex w-full max-w-md flex-col gap-6">
  <h1 class="font-display text-3xl font-bold tracking-tight">{m.onboarding_title()}</h1>
  {#if data.state === 'unavailable'}
    <Alert.Root role="alert">
      <Alert.Title>{m.session_unavailable_title()}</Alert.Title>
      <Alert.Description><p>{m.session_unavailable_text()}</p></Alert.Description>
    </Alert.Root>
  {:else}
    <p class="text-muted-foreground">{m.onboarding_lead()}</p>
    <div class="flex flex-col gap-1 rounded-lg border border-border p-4">
      <span class="text-sm text-muted-foreground">{m.onboarding_current()}</span>
      <!-- The alias is data, not a message. -->
      <span class="font-display text-xl font-semibold break-words" data-testid="onboarding-alias">
        {data.displayName}
      </span>
    </div>
    <DisplayNameForm
      next={data.next}
      value={form?.value ?? ''}
      errorKey={form?.code !== undefined ? resolveApiError(form.code) : null}
    />
    <Button href={data.next} variant="ghost" class="self-start" data-testid="onboarding-skip">
      {m.onboarding_skip()}
    </Button>
  {/if}
</section>
