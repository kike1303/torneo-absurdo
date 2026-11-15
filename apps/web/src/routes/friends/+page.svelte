<script lang="ts">
  import FriendFightList from '#lib/components/friends/FriendFightList.svelte';
  import FriendList from '#lib/components/friends/FriendList.svelte';
  import InviteLink from '#lib/components/friends/InviteLink.svelte';
  import TournamentForm from '#lib/components/friends/TournamentForm.svelte';
  import TournamentList from '#lib/components/friends/TournamentList.svelte';
  import * as Alert from '#lib/components/ui/alert/index.js';
  import { Button } from '#lib/components/ui/button/index.js';
  import { resolveApiError } from '#lib/features/auth/error-text.js';
  import { inviteLink } from '#lib/features/friends/form.js';
  import { pvpFighterName } from '#lib/features/pvp/fighter.js';
  import { onMount } from 'svelte';
  import { invalidateAll } from '$app/navigation';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';
  import PvpReplayScreen from '../pvp/PvpReplayScreen.svelte';

  let { data, form } = $props();
  const locale = $derived(getLocale());

  const SAVED = {
    rotated: m.friends_saved_rotated,
    removed: m.friends_saved_removed,
  } as const;

  // Each action answers one of these, and every other field of the union is `undefined`.
  const errorText = $derived(form?.code === undefined ? null : m[resolveApiError(form.code)]());
  const saved = $derived(form?.saved ?? null);
  const challenged = $derived(form?.challenged ?? null);

  // The key of the next challenge. The load gives one; every challenge that went through takes a
  // new one at once, so a second challenge (to anybody) before the reload is never answered with
  // the first fight. A failed or unanswered one keeps its key: its retry is the same fight.
  let freshKey = $state<string | null>(null);
  let keyedFor = $state<number | null>(null);
  $effect(() => {
    const id = challenged?.id ?? null;
    if (id === null || id === keyedFor) return;
    keyedFor = id;
    freshKey = crypto.randomUUID();
  });
  const clientKey = $derived(freshKey ?? (data.state === 'ready' ? data.clientKey : ''));

  // The link is built in the browser, with its own origin.
  let origin = $state('');
  onMount(() => {
    origin = window.location.origin;
  });
  const link = $derived.by((): string | null => {
    if (data.state !== 'ready' || data.view.invite === null) return null;
    return origin === '' ? '' : inviteLink(origin, locale, data.view.invite);
  });

  // A challenge does not reload the page data: the list of challenges would tell who won before the
  // replay does (011 RF20). It reloads, with a new key, once the result is seen.
  let seen = $state<number | null>(null);
  function onResultSeen(id: number): void {
    if (seen === id) return;
    seen = id;
    void invalidateAll();
  }
</script>

<svelte:head>
  <title>{m.friends_title()} · {m.app_name()}</title>
</svelte:head>

<section class="flex flex-col gap-6" data-testid="friends-page" data-state={data.state}>
  <div class="flex flex-col gap-1">
    <h1 class="font-display text-2xl font-bold tracking-tight sm:text-4xl">{m.friends_title()}</h1>
    <p class="max-w-prose text-muted-foreground">{m.friends_lead()}</p>
  </div>

  {#if errorText !== null}
    <Alert.Root role="alert" variant="destructive" data-testid="friends-error">
      <Alert.Description><p>{errorText}</p></Alert.Description>
    </Alert.Root>
  {:else if saved !== null}
    <p role="status" class="text-sm text-muted-foreground" data-testid="friends-saved">
      {SAVED[saved]()}
    </p>
  {/if}

  {#if data.state === 'unavailable'}
    <Alert.Root role="alert">
      <Alert.Title>{m.session_unavailable_title()}</Alert.Title>
      <Alert.Description><p>{m.session_unavailable_text()}</p></Alert.Description>
    </Alert.Root>
  {:else if data.state === 'error'}
    <div class="flex flex-col items-start gap-3" data-testid="friends-load-error">
      <p role="status">{m.friends_load_error()}</p>
      <Button variant="outline" href={`/${locale}/friends`} data-sveltekit-reload>
        {m.leaderboard_retry()}
      </Button>
    </div>
  {:else if data.state === 'none'}
    <div class="flex flex-col items-start gap-3">
      <p>{m.friends_need_botarga()}</p>
      <Button class="min-h-11" href={`/${locale}/botarga`} data-testid="friends-to-botarga">
        {m.botarga_create()}
      </Button>
    </div>
  {:else}
    {#if challenged !== null}
      <section
        class="flex flex-col gap-3"
        aria-labelledby="challenge-title"
        data-testid="friend-challenge-fight"
      >
        <h2 id="challenge-title" class="font-display text-xl font-semibold">
          {m.replay_versus({
            a: pvpFighterName(challenged.a, locale),
            b: pvpFighterName(challenged.b, locale),
          })}
        </h2>
        {#key challenged.id}
          <PvpReplayScreen
            id={challenged.id}
            kind="friends"
            {locale}
            onresultseen={() => onResultSeen(challenged.id)}
          />
        {/key}
        <div>
          <Button
            variant="link"
            class="px-0"
            href={`/${locale}/friends/fights/${challenged.id}`}
            data-testid="friend-fight-link"
          >
            {m.friends_fight_link()}
          </Button>
        </div>
      </section>
    {/if}

    <div class="flex flex-col gap-6 lg:grid lg:grid-cols-2 lg:items-start">
      <div class="flex min-w-0 flex-col gap-6">
        <InviteLink {link} />
        <FriendList friends={data.view.friends} {clientKey} {locale} />
      </div>
      <div class="flex min-w-0 flex-col gap-6">
        <TournamentForm friends={data.view.friends} clientKey={data.tournamentKey} {locale} />
        <TournamentList
          tournaments={data.tournaments}
          nextBefore={data.tournamentsBefore}
          {locale}
        />
        <FriendFightList fights={data.fights} nextBefore={data.nextBefore} {locale} />
      </div>
    </div>
  {/if}

  <div>
    <Button variant="link" class="px-0" href={`/${locale}/botarga`}>{m.botarga_title()}</Button>
  </div>
</section>
