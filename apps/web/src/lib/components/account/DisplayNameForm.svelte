<script lang="ts">
  import { enhance } from '$app/forms';
  import { Button } from '#lib/components/ui/button/index.js';
  import { Input } from '#lib/components/ui/input/index.js';
  import { Label } from '#lib/components/ui/label/index.js';
  import type { ErrorTextKey } from '#lib/features/auth/error-text.js';
  import { m } from '#lib/paraglide/messages.js';

  // The rename form of the onboarding and the account (008 RF-C4, RF-D5): a form action with
  // progressive enhancement. The error is tied to the field and gets the focus.
  let {
    action = '?/rename',
    value = '',
    errorKey = null,
    next,
    id = 'display-name',
  }: {
    action?: string;
    value?: string;
    errorKey?: ErrorTextKey | null;
    next?: string;
    id?: string;
  } = $props();

  let pending = $state(false);
  let input = $state<HTMLInputElement | null>(null);
  const describedBy = $derived(errorKey ? `${id}-help ${id}-error` : `${id}-help`);
</script>

<form
  method="post"
  {action}
  class="grid gap-3"
  use:enhance={() => {
    pending = true;
    return async ({ update, result }) => {
      await update({ reset: false });
      pending = false;
      if (result.type === 'failure') input?.focus();
    };
  }}
>
  {#if next}
    <input type="hidden" name="next" value={next} />
  {/if}
  <div class="grid gap-2">
    <Label for={id}>{m.display_name_label()}</Label>
    <Input
      bind:ref={input}
      {id}
      name="display_name"
      {value}
      required
      maxlength={64}
      autocomplete="nickname"
      aria-invalid={errorKey ? 'true' : undefined}
      aria-describedby={describedBy}
    />
    <p id={`${id}-help`} class="text-sm text-muted-foreground">{m.display_name_help()}</p>
    {#if errorKey}
      <p
        id={`${id}-error`}
        class="rounded-md bg-destructive px-2 py-1 text-sm text-destructive-foreground"
        data-testid="display-name-error"
      >
        {m[errorKey]()}
      </p>
    {/if}
  </div>
  <Button type="submit" class="w-full sm:w-auto" disabled={pending} data-testid="display-name-save">
    {pending ? m.form_saving() : m.display_name_save()}
  </Button>
</form>
