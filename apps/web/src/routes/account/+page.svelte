<script lang="ts">
  import DisplayNameForm from '#lib/components/account/DisplayNameForm.svelte';
  import ProviderList from '#lib/components/account/ProviderList.svelte';
  import * as Alert from '#lib/components/ui/alert/index.js';
  import { Button } from '#lib/components/ui/button/index.js';
  import * as Card from '#lib/components/ui/card/index.js';
  import { isProvider, linkPanel, providerRows } from '#lib/features/account/providers.js';
  import { resolveApiError } from '#lib/features/auth/error-text.js';
  import { formatCoins } from '#lib/i18n/format.js';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';

  let { data, form } = $props();
  const locale = $derived(getLocale());

  const rows = $derived(
    data.state === 'ready' ? providerRows(data.linked, data.enabled, data.reauth) : [],
  );
  const panel = $derived(
    data.state === 'ready' ? linkPanel(data.reauth, data.linked, data.enabled) : [],
  );
  const renameError = $derived(
    form && 'value' in form && form.code !== undefined ? resolveApiError(form.code) : null,
  );
  const providerError = $derived(
    form && 'provider' in form && isProvider(form.provider)
      ? { provider: form.provider, key: resolveApiError(form.code), code: form.code }
      : null,
  );
  const renameValue = $derived(renameError && form && 'value' in form ? String(form.value) : '');
  // What an action just did goes first: right after unlinking, the page may still have ?reauth=1.
  const status = $derived(
    form && 'renamed' in form
      ? m.account_renamed()
      : form && 'unlinked' in form
        ? m.account_unlinked()
        : data.state === 'ready' && data.reauthDone
          ? m.account_reauth_done()
          : '',
  );
</script>

<svelte:head>
  <title>{m.account_title()} · {m.app_name()}</title>
</svelte:head>

<section class="mx-auto flex w-full max-w-md flex-col gap-6">
  <h1 class="font-display text-3xl font-bold tracking-tight">{m.account_title()}</h1>
  <p role="status" class="text-sm empty:hidden" data-testid="account-status">{status}</p>

  {#if data.state === 'unavailable'}
    <Alert.Root role="alert">
      <Alert.Title>{m.session_unavailable_title()}</Alert.Title>
      <Alert.Description><p>{m.session_unavailable_text()}</p></Alert.Description>
    </Alert.Root>
  {:else}
    <Card.Root>
      <Card.Header>
        <Card.Title><h2>{m.account_name_heading()}</h2></Card.Title>
      </Card.Header>
      <Card.Content class="flex flex-col gap-4">
        <!-- The alias is data, not a message. -->
        <p class="font-display text-xl font-semibold break-words" data-testid="account-alias">
          {data.displayName}
        </p>
        <DisplayNameForm value={renameValue} errorKey={renameError} />
      </Card.Content>
    </Card.Root>

    <Card.Root>
      <Card.Header>
        <Card.Title><h2>{m.account_balance_heading()}</h2></Card.Title>
      </Card.Header>
      <Card.Content>
        <p class="text-2xl font-semibold tabular-nums" data-testid="account-balance">
          {m.coins_amount({ amount: formatCoins(data.balance, locale) })}
        </p>
      </Card.Content>
    </Card.Root>

    <Card.Root>
      <Card.Header>
        <Card.Title><h2>{m.account_providers_heading()}</h2></Card.Title>
      </Card.Header>
      <Card.Content>
        <ProviderList {rows} {locale} error={providerError} linkPanel={panel} />
      </Card.Content>
    </Card.Root>

    <Card.Root>
      <Card.Header>
        <Card.Title><h2>{m.account_sessions_heading()}</h2></Card.Title>
      </Card.Header>
      <Card.Content class="flex flex-col gap-3">
        <!-- Straight to /api: the server expires the session cookie and always answers 303. -->
        <form method="post" action="/api/auth/logout">
          <Button type="submit" variant="outline" class="w-full" data-testid="account-logout">
            {m.account_logout()}
          </Button>
        </form>
        <form method="post" action="/api/auth/logout-all" class="flex flex-col gap-2">
          <Button
            type="submit"
            variant="destructive"
            class="w-full"
            aria-describedby="logout-all-help"
            data-testid="account-logout-all"
          >
            {m.account_logout_all()}
          </Button>
          <p id="logout-all-help" class="text-sm text-muted-foreground">
            {m.account_logout_all_help()}
          </p>
        </form>
      </Card.Content>
    </Card.Root>
  {/if}
</section>
