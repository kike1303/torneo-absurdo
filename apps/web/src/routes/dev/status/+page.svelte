<script lang="ts">
  import { onMount } from 'svelte';
  import { Badge } from '#lib/components/ui/badge/index.js';
  import { Button } from '#lib/components/ui/button/index.js';
  import * as Card from '#lib/components/ui/card/index.js';
  import { Input } from '#lib/components/ui/input/index.js';
  import { Label } from '#lib/components/ui/label/index.js';
  import { Skeleton } from '#lib/components/ui/skeleton/index.js';
  import {
    type CloseReasonKey,
    type EchoConnection,
    type EchoStatus,
    openEcho,
  } from '#lib/features/system/echo.js';
  import { type HealthState, fetchHealth } from '#lib/features/system/health.js';
  import { m } from '#lib/paraglide/messages.js';

  // Behind the /dev/** guard (RF-A16). Everything runs in the browser, on mount: in 001 the SSR
  // never calls the API. The received payload is rendered only with text interpolation.

  let health = $state<HealthState | null>(null);
  let echoStatus = $state<EchoStatus>('connecting');
  let closeCode = $state<number | null>(null);
  let closeReason = $state<CloseReasonKey | null>(null);
  let lastPayload = $state<string | null>(null);
  let draft = $state('hola');
  let connection: EchoConnection | null = null;

  const canSend = $derived(
    echoStatus === 'connected' || echoStatus === 'sent' || echoStatus === 'received',
  );

  async function refresh() {
    health = null;
    health = await fetchHealth();
  }

  function connect() {
    connection?.close();
    closeCode = null;
    closeReason = null;
    lastPayload = null;
    connection = openEcho({
      onStatus: (status) => (echoStatus = status),
      onMessage: (payload) => (lastPayload = payload),
      onClose: (code, reason) => {
        closeCode = code;
        closeReason = reason;
      },
    });
  }

  function send(event: SubmitEvent) {
    event.preventDefault();
    connection?.send(draft);
  }

  function echoStatusText(status: EchoStatus): string {
    switch (status) {
      case 'connecting':
        return m.dev_status_echo_status_connecting();
      case 'connected':
        return m.dev_status_echo_status_connected();
      case 'sent':
        return m.dev_status_echo_status_sent();
      case 'received':
        return m.dev_status_echo_status_received();
      case 'closed':
        return m.dev_status_echo_status_closed({ code: String(closeCode ?? 0) });
    }
  }

  onMount(() => {
    void refresh();
    connect();
    return () => connection?.close();
  });
</script>

<svelte:head>
  <title>{m.dev_status_title()}</title>
</svelte:head>

<div class="flex flex-col gap-6" data-testid="dev-status">
  <div class="flex flex-col gap-2">
    <h1 class="font-display text-3xl font-bold tracking-tight">{m.dev_status_title()}</h1>
    <p class="text-muted-foreground">{m.dev_status_lead()}</p>
  </div>

  <Card.Root>
    <Card.Header>
      <Card.Title>{m.dev_status_server_heading()}</Card.Title>
    </Card.Header>
    <Card.Content class="flex flex-wrap items-center gap-4">
      <div aria-live="polite" class="flex min-h-11 items-center" data-testid="server-status">
        {#if health === null}
          <Skeleton class="h-5 w-40" aria-hidden="true" />
          <span class="sr-only">{m.dev_status_server_loading()}</span>
        {:else if health.kind === 'ok'}
          <Badge data-state="ok">{m.dev_status_server_ok({ version: health.version })}</Badge>
        {:else if health.kind === 'db_unavailable'}
          <Badge variant="destructive" data-state="db_unavailable">
            {m.dev_status_server_db_unavailable()}
          </Badge>
        {:else}
          <Badge variant="outline" data-state="network_error">
            {m.dev_status_server_network_error()}
          </Badge>
        {/if}
      </div>
      <Button variant="outline" onclick={refresh}>{m.dev_status_server_retry()}</Button>
    </Card.Content>
  </Card.Root>

  <Card.Root>
    <Card.Header>
      <Card.Title>{m.dev_status_echo_heading()}</Card.Title>
    </Card.Header>
    <Card.Content class="flex flex-col gap-4">
      <form class="flex flex-col gap-3 sm:flex-row sm:items-end" onsubmit={send}>
        <div class="flex flex-1 flex-col gap-2">
          <Label for="echo-input">{m.dev_status_echo_input_label()}</Label>
          <Input id="echo-input" bind:value={draft} maxlength={1000} autocomplete="off" />
        </div>
        <Button type="submit" disabled={!canSend}>{m.dev_status_echo_send()}</Button>
        <Button type="button" variant="outline" onclick={connect}>
          {m.dev_status_echo_reconnect()}
        </Button>
      </form>
      <div aria-live="polite" class="flex flex-col gap-1" data-testid="echo-status">
        <p data-state={echoStatus}>{echoStatusText(echoStatus)}</p>
        {#if closeReason !== null}
          <p class="text-muted-foreground" data-testid="echo-close-reason">{m[closeReason]()}</p>
        {/if}
        {#if lastPayload !== null}
          <p>
            <span class="text-muted-foreground">{m.dev_status_echo_received_label()}</span>
            <span data-testid="echo-payload">{lastPayload}</span>
          </p>
        {/if}
      </div>
    </Card.Content>
  </Card.Root>
</div>
