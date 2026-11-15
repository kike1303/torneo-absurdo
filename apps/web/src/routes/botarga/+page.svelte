<script lang="ts">
  import BotargaPreview from '#lib/components/botarga/BotargaPreview.svelte';
  import BotargaProgress from '#lib/components/botarga/BotargaProgress.svelte';
  import BuildForm from '#lib/components/botarga/BuildForm.svelte';
  import DrawPanel from '#lib/components/botarga/DrawPanel.svelte';
  import SearchFight from '#lib/components/botarga/SearchFight.svelte';
  import { SHOWCASE } from '#lib/features/botarga/showcase.js';
  import { buildChoices, drawOptions } from '#lib/features/botarga/view.js';
  import PvpReplayScreen from '../pvp/PvpReplayScreen.svelte';
  import PvpRewardCard from '#lib/components/pvp/PvpRewardCard.svelte';
  import * as Alert from '#lib/components/ui/alert/index.js';
  import { Button } from '#lib/components/ui/button/index.js';
  import { resolveApiError } from '#lib/features/auth/error-text.js';
  import { pvpFighterName } from '#lib/features/pvp/fighter.js';
  import { invalidateAll } from '$app/navigation';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';

  let { data, form } = $props();
  const locale = $derived(getLocale());

  const SAVED = {
    created: m.botarga_saved_created,
    build: m.botarga_saved_build,
    draw: m.botarga_saved_draw,
  } as const;

  // Each action answers one of these, and every other field of the union is `undefined`.
  const errorText = $derived(form?.code === undefined ? null : m[resolveApiError(form.code)]());
  const saved = $derived(form?.saved ?? null);
  const attacked = $derived(form?.attacked ?? null);
  const draw = $derived(data.state === 'ready' ? (data.botarga.draws[0] ?? null) : null);

  // The fight whose result has been seen (owner, 2026-11-14): its reward, and the level, the XP and
  // the draw it changed, show only then, as the result itself (011 RF20). Then the page data is
  // read again, with a new key for the next search.
  let seen = $state<number | null>(null);

  // What the form has on, before saving: the 3D preview follows it.
  let previewWear = $state<readonly string[] | null>(null);
  const wear = $derived(
    previewWear ?? (data.state === 'ready' ? data.botarga.build.accessories : []),
  );
  const resultSeen = $derived(attacked !== null && seen === attacked.id);
  function onResultSeen(id: number): void {
    if (seen === id) return;
    seen = id;
    void invalidateAll();
  }
</script>

<svelte:head>
  <title>{m.botarga_title()} · {m.app_name()}</title>
</svelte:head>

<section class="flex flex-col gap-6" data-testid="botarga-page" data-state={data.state}>
  <div class="flex flex-col gap-1">
    <h1 class="font-display text-2xl font-bold tracking-tight sm:text-4xl">{m.botarga_title()}</h1>
    <p class="max-w-prose text-muted-foreground">{m.botarga_lead()}</p>
  </div>

  {#if errorText !== null}
    <Alert.Root role="alert" variant="destructive" data-testid="botarga-error">
      <Alert.Description><p>{errorText}</p></Alert.Description>
    </Alert.Root>
  {:else if saved !== null}
    <p role="status" class="text-sm text-muted-foreground" data-testid="botarga-saved">
      {SAVED[saved]()}
    </p>
  {/if}

  {#if data.state === 'unavailable'}
    <Alert.Root role="alert">
      <Alert.Title>{m.session_unavailable_title()}</Alert.Title>
      <Alert.Description><p>{m.session_unavailable_text()}</p></Alert.Description>
    </Alert.Root>
  {:else if data.state === 'error'}
    <div class="flex flex-col items-start gap-3" data-testid="botarga-load-error">
      <p role="status">{m.botarga_load_error()}</p>
      <Button variant="outline" href={`/${locale}/botarga`} data-sveltekit-reload>
        {m.leaderboard_retry()}
      </Button>
    </div>
  {:else if data.state === 'none'}
    <form method="post" action="?/create" class="flex flex-col items-start gap-3">
      <p>{m.botarga_none_text()}</p>
      <Button type="submit" class="min-h-11" data-testid="botarga-create"
        >{m.botarga_create()}</Button
      >
    </form>
  {:else}
    {@const botarga = data.botarga}
    {#if attacked !== null}
      <section class="flex flex-col gap-3" aria-labelledby="attack-title" data-testid="pvp-attack">
        <h2 id="attack-title" class="font-display text-xl font-semibold">
          {m.replay_versus({
            a: pvpFighterName(attacked.a, locale),
            b: pvpFighterName(attacked.b, locale),
          })}
        </h2>
        {#key attacked.id}
          <PvpReplayScreen
            id={attacked.id}
            {locale}
            onresultseen={() => onResultSeen(attacked.id)}
          />
        {/key}
        {#if resultSeen}
          <PvpRewardCard reward={attacked.reward} {locale} />
        {/if}
        <div>
          <Button variant="link" class="px-0" href={`/${locale}/pvp/fights/${attacked.id}`}>
            {m.pvp_fight_link()}
          </Button>
        </div>
      </section>
    {/if}

    {#if draw !== null}
      <DrawPanel
        {draw}
        options={drawOptions(draw, locale)}
        queued={botarga.queued_draws}
        {locale}
      />
    {/if}

    <div
      class="flex flex-col gap-6 lg:grid lg:grid-cols-[minmax(0,3fr)_minmax(0,2fr)] lg:items-start"
    >
      <div class="flex min-w-0 flex-col gap-4">
        {#key `${botarga.build.cosmetics.idle}/${botarga.build.cosmetics.victory}/${wear.join(',')}`}
          <BotargaPreview
            name={m.botarga_title()}
            cosmetics={botarga.build.cosmetics}
            {wear}
            source={() => Promise.resolve(SHOWCASE)}
          />
        {/key}
      </div>
      <div class="flex min-w-0 flex-col gap-4">
        <BotargaProgress {botarga} {locale} />
        <SearchFight clientKey={data.clientKey} />
        <div class="flex flex-wrap gap-2">
          <Button variant="outline" href={`/${locale}/pvp/fights`} data-testid="botarga-my-fights">
            {m.pvp_fights_title()}
          </Button>
          <Button variant="outline" href={`/${locale}/pvp/leaderboard`}>
            {m.pvp_leaderboard_title()}
          </Button>
        </div>
      </div>
    </div>

    {#if draw === null}
      {#key JSON.stringify(botarga.build)}
        <BuildForm
          {botarga}
          choices={buildChoices(botarga, locale)}
          {locale}
          onwear={(ids) => (previewWear = ids)}
        />
      {/key}
    {:else}
      <p class="text-sm text-muted-foreground" data-testid="botarga-build-waits">
        {m.botarga_build_waits()}
      </p>
    {/if}
  {/if}
</section>
