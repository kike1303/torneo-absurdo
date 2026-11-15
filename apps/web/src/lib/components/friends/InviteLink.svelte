<script lang="ts">
  // My invite link (023): copy it, or change it so the old one stops working. The code goes in the
  // fragment of the link, so it never reaches a server log.
  import { enhance } from '$app/forms';
  import { Button } from '#lib/components/ui/button/index.js';
  import { Input } from '#lib/components/ui/input/index.js';
  import { m } from '#lib/paraglide/messages.js';

  interface Props {
    /** Null: no code yet. Empty: the browser has not built it yet. */
    link: string | null;
  }

  let { link }: Props = $props();

  let copied = $state<'copied' | 'failed' | null>(null);
  async function copy(text: string): Promise<void> {
    if (text === '') return;
    try {
      await navigator.clipboard.writeText(text);
      copied = 'copied';
    } catch {
      copied = 'failed';
    }
  }
  let pending = $state(false);
</script>

<section
  class="flex flex-col gap-3 rounded-lg border border-border bg-card p-4 text-card-foreground"
  aria-labelledby="invite-title"
  data-testid="friends-invite"
>
  <div class="flex flex-col gap-1">
    <h2 id="invite-title" class="font-display text-lg font-semibold">
      {m.friends_invite_title()}
    </h2>
    <p class="text-sm text-muted-foreground">{m.friends_invite_lead()}</p>
  </div>
  {#if link === null}
    <p>{m.friends_invite_none()}</p>
  {:else}
    <div class="flex flex-col gap-2 sm:flex-row">
      <label class="sr-only" for="invite-link">{m.friends_invite_label()}</label>
      <Input
        id="invite-link"
        readonly
        value={link}
        class="min-h-11 font-mono text-sm"
        data-testid="friends-invite-link"
        onfocus={(event) => (event.currentTarget as HTMLInputElement).select()}
      />
      <Button
        type="button"
        class="min-h-11"
        onclick={() => void copy(link)}
        data-testid="friends-invite-copy"
      >
        {m.friends_invite_copy()}
      </Button>
    </div>
    {#if copied !== null}
      <p role="status" class="text-sm text-muted-foreground">
        {copied === 'copied' ? m.friends_invite_copied() : m.friends_invite_copy_failed()}
      </p>
    {/if}
  {/if}
  <form
    method="post"
    action="?/rotate"
    class="flex flex-col items-start gap-1"
    use:enhance={() => {
      pending = true;
      copied = null;
      return async ({ update }) => {
        await update();
        pending = false;
      };
    }}
  >
    <Button
      type="submit"
      variant="outline"
      class="min-h-11"
      disabled={pending}
      data-testid="friends-invite-rotate"
    >
      {link === null ? m.friends_invite_create() : m.friends_invite_rotate()}
    </Button>
    {#if link !== null}
      <p class="text-sm text-muted-foreground">{m.friends_invite_rotate_lead()}</p>
    {/if}
  </form>
</section>
