<script lang="ts">
  import { Input } from '#lib/components/ui/input/index.js';
  import { Label } from '#lib/components/ui/label/index.js';
  import { m } from '#lib/paraglide/messages.js';

  // `dev_identity` of the development provider (008 RF-B6): the same rule as the server.
  let { id = 'dev-identity', value = '' }: { id?: string; value?: string } = $props();
</script>

<div class="grid gap-2">
  <Label for={id}>{m.login_dev_identity_label()}</Label>
  <Input
    {id}
    name="dev_identity"
    {value}
    required
    pattern={'[a-z0-9_\\-]{1,32}'}
    maxlength={32}
    autocomplete="username"
    autocapitalize="none"
    spellcheck={false}
    aria-describedby={`${id}-help`}
  />
  <p id={`${id}-help`} class="text-sm text-muted-foreground">{m.login_dev_identity_help()}</p>
</div>
