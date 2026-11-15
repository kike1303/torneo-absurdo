<script lang="ts">
  // An invite (023 "Web"). The code comes in the fragment (`#<code>`), and the tab keeps it in its
  // `sessionStorage` across the login and the creation of a botarga. Without a session the page says
  // only that there is an invite. With one and a botarga, it asks the server whose invite it is and
  // waits for a click on "accept": it never accepts on its own.
  import { onMount, tick } from 'svelte';
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import * as Alert from '#lib/components/ui/alert/index.js';
  import { Button } from '#lib/components/ui/button/index.js';
  import { resolveApiError } from '#lib/features/auth/error-text.js';
  import { codeFromHash } from '#lib/features/friends/form.js';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';

  let { data, form } = $props();
  const locale = $derived(getLocale());

  const STORAGE_KEY = 'torneo.friend_invite';

  /** `undefined` until the browser has read the fragment and the storage. */
  let code = $state<string | null | undefined>(undefined);
  onMount(() => {
    const fromHash = codeFromHash(window.location.hash);
    try {
      if (fromHash !== null) window.sessionStorage.setItem(STORAGE_KEY, fromHash);
      code = fromHash ?? codeFromHash(window.sessionStorage.getItem(STORAGE_KEY) ?? '');
    } catch {
      code = fromHash;
    }
  });

  const errorText = $derived(form?.code === undefined ? null : m[resolveApiError(form.code)]());
  const owner = $derived(form?.preview ?? null);
  const accepted = $derived(form?.accepted ?? null);

  $effect(() => {
    if (accepted === null) return;
    try {
      window.sessionStorage.removeItem(STORAGE_KEY);
    } catch {
      // Nothing kept, nothing to forget.
    }
  });

  // The preview is asked once per code, as soon as the reader can accept.
  let previewForm = $state<HTMLFormElement | null>(null);
  let asked = $state(false);
  $effect(() => {
    if (data.state !== 'ready' || typeof code !== 'string' || asked || previewForm === null) return;
    asked = true;
    void tick().then(() => previewForm?.requestSubmit());
  });

  let pending = $state(false);
  const login = $derived(
    `/${locale}/login?next=${encodeURIComponent(`/${locale}/friends/invite`)}`,
  );
</script>

<svelte:head>
  <title>{m.friends_invite_page_title()} · {m.app_name()}</title>
</svelte:head>

<section
  class="flex max-w-prose flex-col gap-4"
  data-testid="friend-invite-page"
  data-state={data.state}
>
  <h1 class="font-display text-2xl font-bold tracking-tight sm:text-4xl">
    {m.friends_invite_page_title()}
  </h1>

  {#if data.state === 'unavailable'}
    <Alert.Root role="alert">
      <Alert.Title>{m.session_unavailable_title()}</Alert.Title>
      <Alert.Description><p>{m.session_unavailable_text()}</p></Alert.Description>
    </Alert.Root>
  {:else if code === null}
    <p data-testid="friend-invite-missing">{m.friends_invite_missing()}</p>
  {:else if data.state === 'anonymous'}
    <p>{m.friends_invite_page_text()}</p>
    <div>
      <Button class="min-h-11" href={login} data-testid="friend-invite-login">
        {m.header_sign_in()}
      </Button>
    </div>
  {:else if data.state === 'error'}
    <div class="flex flex-col items-start gap-3">
      <p role="status">{m.friends_load_error()}</p>
      <Button variant="outline" href={`/${locale}/friends/invite`} data-sveltekit-reload>
        {m.leaderboard_retry()}
      </Button>
    </div>
  {:else if data.state === 'none'}
    <form
      method="post"
      action="?/create"
      class="flex flex-col items-start gap-3"
      use:enhance={() => {
        pending = true;
        return async ({ update }) => {
          await update();
          await invalidateAll();
          pending = false;
        };
      }}
    >
      <p>{m.friends_invite_need_botarga()}</p>
      <Button type="submit" class="min-h-11" disabled={pending} data-testid="friend-invite-create">
        {m.botarga_create()}
      </Button>
    </form>
  {:else if accepted !== null}
    <p role="status" data-testid="friend-invite-accepted">
      {m.friends_invite_accepted({ name: accepted })}
    </p>
    <div>
      <Button class="min-h-11" href={`/${locale}/friends`} data-testid="friend-invite-go">
        {m.friends_go()}
      </Button>
    </div>
  {:else}
    {#if errorText !== null}
      <Alert.Root role="alert" variant="destructive" data-testid="friend-invite-error">
        <Alert.Description><p>{errorText}</p></Alert.Description>
      </Alert.Root>
    {/if}
    <form
      method="post"
      action="?/preview"
      bind:this={previewForm}
      use:enhance={() => {
        return async ({ update }) => {
          await update({ reset: false, invalidateAll: false });
        };
      }}
    >
      <input type="hidden" name="code" value={code ?? ''} />
    </form>
    {#if owner !== null}
      <form
        method="post"
        action="?/accept"
        class="flex flex-col items-start gap-3"
        use:enhance={() => {
          pending = true;
          return async ({ update }) => {
            await update({ reset: false, invalidateAll: false });
            pending = false;
          };
        }}
      >
        <p data-testid="friend-invite-owner">{m.friends_invite_from({ name: owner })}</p>
        <input type="hidden" name="code" value={code ?? ''} />
        <input type="hidden" name="display_name" value={owner} />
        <Button
          type="submit"
          size="lg"
          class="min-h-11"
          disabled={pending}
          data-testid="friend-invite-accept"
        >
          {m.friends_invite_accept()}
        </Button>
      </form>
    {:else if errorText === null}
      <p role="status" class="text-muted-foreground">{m.friends_invite_loading()}</p>
    {/if}
  {/if}

  {#if data.state !== 'anonymous'}
    <div>
      <Button variant="link" class="px-0" href={`/${locale}/friends`}>{m.friends_title()}</Button>
    </div>
  {/if}
</section>
