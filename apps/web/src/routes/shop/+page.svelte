<script lang="ts">
  import BotargaPreview from '#lib/components/botarga/BotargaPreview.svelte';
  import ShopConfirm from '#lib/components/shop/ShopConfirm.svelte';
  import ShopList from '#lib/components/shop/ShopList.svelte';
  import * as Alert from '#lib/components/ui/alert/index.js';
  import { Button } from '#lib/components/ui/button/index.js';
  import { resolveApiError } from '#lib/features/auth/error-text.js';
  import { SHOWCASE } from '#lib/features/botarga/showcase.js';
  import { shopSections, tryOn } from '#lib/features/shop/shop.js';
  import { formatCoins } from '#lib/i18n/format.js';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';

  let { data, form } = $props();
  const locale = $derived(getLocale());

  const errorText = $derived(form?.code === undefined ? null : m[resolveApiError(form.code)]());
  const bought = $derived(form?.bought ?? null);

  const sections = $derived(data.state === 'ready' ? shopSections(data.shop, locale) : []);
  const picked = $derived(data.state === 'ready' ? data.picked : null);
  const entry = $derived(
    picked === null
      ? null
      : (sections
          .flatMap((s) => s.entries)
          .find((e) => e.kind === picked.kind && e.id === picked.id) ?? null),
  );
  const look = $derived(data.state === 'ready' ? tryOn(data.botarga.build, picked) : null);
</script>

<svelte:head>
  <title>{m.shop_title()} · {m.app_name()}</title>
</svelte:head>

<section class="flex flex-col gap-6" data-testid="shop-page" data-state={data.state}>
  <div class="flex flex-col gap-1">
    <h1 class="font-display text-2xl font-bold tracking-tight sm:text-4xl">{m.shop_title()}</h1>
    <p class="max-w-prose text-muted-foreground">{m.shop_lead()}</p>
  </div>

  {#if errorText !== null}
    <Alert.Root role="alert" variant="destructive" data-testid="shop-error">
      <Alert.Description><p>{errorText}</p></Alert.Description>
    </Alert.Root>
  {:else if bought !== null}
    <div role="status" class="flex flex-wrap items-center gap-3" data-testid="shop-bought">
      <p>{m.shop_bought()}</p>
      <Button variant="link" class="min-h-11 px-0" href={`/${locale}/botarga`}>
        {m.shop_to_botarga()}
      </Button>
    </div>
  {/if}

  {#if data.state === 'unavailable'}
    <Alert.Root role="alert">
      <Alert.Title>{m.session_unavailable_title()}</Alert.Title>
      <Alert.Description><p>{m.session_unavailable_text()}</p></Alert.Description>
    </Alert.Root>
  {:else if data.state === 'error'}
    <div class="flex flex-col items-start gap-3" data-testid="shop-load-error">
      <p role="status">{m.shop_load_error()}</p>
      <Button variant="outline" href={`/${locale}/shop`} data-sveltekit-reload>
        {m.shop_retry()}
      </Button>
    </div>
  {:else if data.state === 'none'}
    <div class="flex flex-col items-start gap-3">
      <p>{m.shop_none_text()}</p>
      <Button class="min-h-11" href={`/${locale}/botarga`}>{m.shop_to_botarga()}</Button>
    </div>
  {:else if look !== null}
    <p class="text-lg font-medium tabular-nums" data-testid="shop-balance">
      {m.shop_balance({ amount: formatCoins(data.shop.balance, locale) })}
    </p>
    <div
      class="flex flex-col gap-6 lg:grid lg:grid-cols-[minmax(0,3fr)_minmax(0,2fr)] lg:items-start"
    >
      <div class="flex min-w-0 flex-col gap-4 lg:sticky lg:top-4">
        {#key `${look.cosmetics.idle}/${look.cosmetics.victory}/${look.wear.join(',')}/${look.skin}`}
          <BotargaPreview
            name={m.botarga_title()}
            cosmetics={look.cosmetics}
            wear={look.wear}
            skin={look.skin}
            source={() => Promise.resolve(SHOWCASE)}
          />
        {/key}
        {#if entry !== null}
          {#key `${entry.kind}:${entry.id}`}
            <ShopConfirm {entry} balance={data.shop.balance} clientKey={data.clientKey} {locale} />
          {/key}
        {:else}
          <p class="text-sm text-muted-foreground" data-testid="shop-pick-hint">
            {m.shop_pick_hint()}
          </p>
        {/if}
      </div>
      <ShopList {sections} {picked} {locale} />
    </div>
  {/if}
</section>
