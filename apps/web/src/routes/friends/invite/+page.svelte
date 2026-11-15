<script lang="ts">
  // An invite (023 "Web"). The code comes in the fragment (`#<code>`), and the tab keeps it in its
  // `sessionStorage` across the login and the creation of a botarga. Without a session the page says
  // only that there is an invite. With one and a botarga, it asks the server whose invite it is and
  // waits for a click on "accept": it never accepts on its own.
  import { onMount, tick } from 'svelte';
  import { enhance } from '$app/forms';
  import { replaceState } from '$app/navigation';
  import { page } from '$app/state';
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
    // Out of the address bar and the history once kept: a shared screen or a synced history must
    // not show it.
    if (fromHash !== null) {
      void tick().then(() => {
        try {
          replaceState(window.location.pathname + window.location.search, page.state);
        } catch {
          // The router is not ready: the fragment stays, as it came.
        }
      });
    }
  });

  const errorText = $derived(form?.code === undefined ? null : m[resolveApiError(form.code)]());
  // Whose invite it is, kept once the server said it: a failed accept leaves it on screen, so the
  // reader can try again without asking for it once more.
  let owner = $state<string | null>(null);
  $effect(() => {
    if (form?.preview !== undefined) owner = form.preview;
  });
  const accepted = $derived(form?.accepted === true);

  /** Forgets the code of this tab: accepted, or one no invite has. */
  function forget(): void {
    try {
      window.sessionStorage.removeItem(STORAGE_KEY);
    } catch {
      // Nothing kept, nothing to forget.
    }
  }
  $effect(() => {
    // Accepted, or refused for good (no such invite, or friends already): another account in this
    // tab must not find it. A busy or limited answer keeps it for a retry.
    if (
      accepted ||
      form?.code === 'friends.invite_not_found' ||
      form?.code === 'friends.already_friends'
    ) {
      forget();
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
          pending = false;
        };
      }}
    >
      <p>{m.friends_invite_need_botarga()}</p>
      <Button type="submit" class="min-h-11" disabled={pending} data-testid="friend-invite-create">
        {m.botarga_create()}
      </Button>
    </form>
  {:else if accepted}
    <p role="status" data-testid="friend-invite-accepted">
      {m.friends_invite_accepted({ name: owner ?? '' })}
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
    {:else if form?.code !== 'friends.invite_not_found'}
      <div>
        <Button
          type="button"
          variant="outline"
          class="min-h-11"
          onclick={() => previewForm?.requestSubmit()}
          data-testid="friend-invite-retry"
        >
          {m.friends_invite_retry()}
        </Button>
      </div>
    {/if}
  {/if}

  {#if data.state !== 'anonymous'}
    <div>
      <Button variant="link" class="px-0" href={`/${locale}/friends`}>{m.friends_title()}</Button>
    </div>
  {/if}
</section>
