<script lang="ts">
  import { enhance } from '$app/forms';
  import type { Provider } from '@torneo/protocol';
  import { Badge } from '#lib/components/ui/badge/index.js';
  import { Button } from '#lib/components/ui/button/index.js';
  import type { ProviderRow } from '#lib/features/account/providers.js';
  import type { ErrorTextKey } from '#lib/features/auth/error-text.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';
  import ReauthPanel from './ReauthPanel.svelte';
  import { providerNames } from './provider-names.js';

  // The providers of the account (008 RF-C2, RF-C3, plan-d decision 9). Linking posts straight
  // to /api (the server sets the flow cookie and redirects to the provider); unlinking is a form
  // action, so its errors arrive as text, never as raw JSON. A missing confirmation opens the
  // panel of 009 RF21; `linkPanel` is the panel that replaces the link buttons (RF20).
  let {
    rows,
    locale,
    error = null,
    linkPanel = [],
    idPrefix = '',
  }: {
    rows: ProviderRow[];
    locale: Locale;
    error?: { provider: Provider; key: ErrorTextKey; code?: unknown } | null;
    linkPanel?: readonly Provider[];
    idPrefix?: string;
  } = $props();

  let pending = $state<Provider | null>(null);

  /** The row whose confirmation the server just asked for, if it has something to offer. */
  const panelRow = $derived(
    error?.code === 'auth.reauth_required'
      ? (rows.find((r) => r.provider === error.provider && r.reauthWith.length > 0) ?? null)
      : null,
  );
</script>

<ul class="grid gap-3">
  {#each rows as row (row.provider)}
    {@const name = providerNames[row.provider]()}
    <li
      class="grid gap-2 rounded-lg border border-border p-3"
      data-testid={`provider-row-${row.provider}`}
    >
      <div class="flex flex-wrap items-center justify-between gap-2">
        <span class="font-medium">{name}</span>
        <Badge variant={row.linked ? 'secondary' : 'outline'}>
          {row.linked ? m.account_provider_linked() : m.account_provider_not_linked()}
        </Badge>
      </div>
      {#if row.blocked}
        <p class="text-sm text-muted-foreground" data-testid={`provider-blocked-${row.provider}`}>
          {m.account_unlink_disabled_hint()}
        </p>
      {:else if row.linked}
        <form
          method="post"
          action="?/unlink"
          use:enhance={() => {
            pending = row.provider;
            return async ({ update }) => {
              await update();
              pending = null;
            };
          }}
        >
          <input type="hidden" name="provider" value={row.provider} />
          <Button
            type="submit"
            variant="outline"
            class="w-full sm:w-auto"
            disabled={!row.canUnlink || pending !== null}
            aria-label={m.account_unlink_label({ provider: name })}
            aria-describedby={!row.canUnlink
              ? `${idPrefix}provider-hint-${row.provider}`
              : undefined}
            data-testid={`unlink-${row.provider}`}
          >
            {m.account_unlink()}
          </Button>
        </form>
        {#if !row.canUnlink}
          <p
            id={`${idPrefix}provider-hint-${row.provider}`}
            class="text-sm text-muted-foreground"
            data-testid={`provider-hint-${row.provider}`}
          >
            {m.account_last_provider_hint()}
          </p>
        {/if}
      {:else if linkPanel.length === 0}
        <form method="post" action={`/api/auth/link/${row.provider}`}>
          <input type="hidden" name="next" value={`/${locale}/account`} />
          <Button
            type="submit"
            class="w-full sm:w-auto"
            aria-label={m.account_link_label({ provider: name })}
            data-testid={`link-${row.provider}`}
          >
            {m.account_link_action()}
          </Button>
        </form>
      {/if}
      {#if panelRow?.provider === row.provider}
        <ReauthPanel
          providers={row.reauthWith}
          {locale}
          {idPrefix}
          testid={`reauth-panel-${row.provider}`}
          autofocus
        />
      {:else if error && error.provider === row.provider}
        <p
          role="alert"
          class="rounded-md bg-destructive px-2 py-1 text-sm text-destructive-foreground"
          data-testid={`provider-error-${row.provider}`}
        >
          {m[error.key]()}
        </p>
      {/if}
    </li>
  {/each}
</ul>
<!-- One panel at a time: the one the user just asked for wins (009 RF14, step 6). -->
{#if panelRow === null && linkPanel.length > 0}
  <div class="mt-3">
    <ReauthPanel providers={linkPanel} {locale} {idPrefix} testid="reauth-panel-link" />
  </div>
{/if}
