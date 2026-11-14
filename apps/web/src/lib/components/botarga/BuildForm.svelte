<script lang="ts">
  // The build of my botarga (021 "Build"): stats within the budget of the level, a style, the
  // attacks to equip (as many as the open slots) and the idle and victory picked. A form action with
  // progressive enhancement: the server checks every rule and answers with a code. The total is a
  // help, never the rule.
  import { onMount } from 'svelte';
  import { enhance } from '$app/forms';
  import type { BotargaView } from '@torneo/protocol';
  import { Badge } from '#lib/components/ui/badge/index.js';
  import { Button } from '#lib/components/ui/button/index.js';
  import * as Card from '#lib/components/ui/card/index.js';
  import { Input } from '#lib/components/ui/input/index.js';
  import { Label } from '#lib/components/ui/label/index.js';
  import type { BuildChoices, ClipLabel } from '#lib/features/botarga/view.js';
  import type { StatKey } from '#lib/features/botarga/catalog.js';
  import { formatInteger } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';

  interface Props {
    botarga: BotargaView;
    choices: BuildChoices;
    locale: Locale;
  }

  let { botarga, choices, locale }: Props = $props();

  const STAT_LABEL: Record<StatKey, () => string> = {
    hp: m.botarga_stat_hp,
    attack: m.botarga_stat_attack,
    defense: m.botarga_stat_defense,
    speed: m.botarga_stat_speed,
    special: m.botarga_stat_special,
  };

  // svelte-ignore state_referenced_locally
  let stats = $state({ ...botarga.build.stats });
  // svelte-ignore state_referenced_locally
  let equipped = $state<string[]>([...botarga.build.abilities]);
  let pending = $state(false);
  // Without JavaScript nothing is disabled: the server checks the slots and answers with a code.
  let hydrated = $state(false);
  onMount(() => {
    hydrated = true;
  });

  const budget = $derived(
    choices.statKeys.reduce((sum, key) => sum + botarga.build.stats[key], 0) +
      botarga.progress.budget_free,
  );
  const used = $derived(choices.statKeys.reduce((sum, key) => sum + (Number(stats[key]) || 0), 0));
  const slots = $derived(botarga.progress.slots_open);

  function clipText(label: ClipLabel): string {
    const n = formatInteger(label.n, locale);
    return label.clip === 'win' ? m.botarga_clip_victory({ n }) : m.botarga_clip_idle({ n });
  }

  function toggle(id: string, on: boolean): void {
    equipped = on ? [...equipped, id] : equipped.filter((x) => x !== id);
  }
</script>

<Card.Root data-testid="botarga-build">
  <Card.Content>
    <form
      method="post"
      action="?/build"
      class="flex flex-col gap-6"
      use:enhance={() => {
        pending = true;
        return async ({ update }) => {
          await update({ reset: false });
          pending = false;
        };
      }}
    >
      <h2 class="font-display text-xl font-semibold">{m.botarga_build_title()}</h2>

      <fieldset class="flex flex-col gap-3">
        <legend class="mb-2 text-sm font-medium">{m.botarga_stats()}</legend>
        <div class="grid grid-cols-2 gap-3 sm:grid-cols-5">
          {#each choices.statKeys as key (key)}
            <div class="grid gap-1">
              <Label for={`stat-${key}`}>{STAT_LABEL[key]()}</Label>
              <Input
                id={`stat-${key}`}
                name={key}
                type="number"
                inputmode="numeric"
                min={choices.statMin}
                step={1}
                required
                class="h-11 tabular-nums"
                bind:value={stats[key]}
              />
            </div>
          {/each}
        </div>
        <p
          class="text-sm tabular-nums"
          class:text-destructive={used > budget}
          class:text-muted-foreground={used <= budget}
          aria-live="polite"
          data-testid="botarga-stats-total"
        >
          {m.botarga_stats_total({
            used: formatInteger(used, locale),
            budget: formatInteger(budget, locale),
          })}
        </p>
      </fieldset>

      <div class="grid gap-2">
        <Label for="style">{m.botarga_style()}</Label>
        <select
          id="style"
          name="style_id"
          class="h-11 rounded-md border border-input bg-background px-3 text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        >
          {#each choices.styles as style (style.id)}
            <option value={style.id} selected={style.id === botarga.build.style_id}
              >{style.name}</option
            >
          {/each}
        </select>
      </div>

      <fieldset class="flex flex-col gap-2">
        <legend class="mb-1 text-sm font-medium">{m.botarga_abilities()}</legend>
        <p class="text-sm text-muted-foreground">
          {m.botarga_abilities_hint({ n: formatInteger(slots, locale) })}
        </p>
        <ul class="flex flex-col gap-2">
          {#each choices.abilities as item (item.id)}
            {@const on = equipped.includes(item.id)}
            <li>
              <label
                class="flex min-h-11 cursor-pointer items-center gap-3 rounded-lg border border-border px-3 py-2 has-[:checked]:border-primary has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-ring"
              >
                <input
                  type="checkbox"
                  name="ability"
                  value={item.id}
                  checked={on}
                  disabled={hydrated && !on && equipped.length >= slots}
                  class="size-5 accent-primary"
                  onchange={(event) => toggle(item.id, event.currentTarget.checked)}
                  data-testid="botarga-ability"
                />
                <span class="flex-1">{item.name}</span>
                {#if item.kind !== null}
                  <Badge variant="outline">
                    {item.kind === 'heavy' ? m.botarga_kind_heavy() : m.botarga_kind_special()}
                  </Badge>
                {/if}
              </label>
            </li>
          {/each}
        </ul>
      </fieldset>

      {#each botarga.build.accessories as id (id)}
        <input type="hidden" name="accessory" value={id} />
      {/each}

      <fieldset class="grid gap-3 sm:grid-cols-2">
        <legend class="mb-1 text-sm font-medium">{m.botarga_cosmetics()}</legend>
        <div class="grid gap-2">
          <Label for="idle">{m.botarga_idle()}</Label>
          <select
            id="idle"
            name="idle"
            class="h-11 rounded-md border border-input bg-background px-3 text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          >
            <option value="" selected={botarga.build.cosmetics.idle === null}>
              {m.botarga_clip_default()}
            </option>
            {#each choices.idles as label (label.code)}
              <option value={label.code} selected={label.code === botarga.build.cosmetics.idle}>
                {clipText(label)}
              </option>
            {/each}
          </select>
        </div>
        <div class="grid gap-2">
          <Label for="victory">{m.botarga_victory()}</Label>
          <select
            id="victory"
            name="victory"
            class="h-11 rounded-md border border-input bg-background px-3 text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          >
            <option value="" selected={botarga.build.cosmetics.victory === null}>
              {m.botarga_clip_default()}
            </option>
            {#each choices.victories as label (label.code)}
              <option value={label.code} selected={label.code === botarga.build.cosmetics.victory}>
                {clipText(label)}
              </option>
            {/each}
          </select>
        </div>
      </fieldset>

      <div>
        <Button type="submit" disabled={pending} class="min-h-11" data-testid="botarga-save">
          {m.botarga_save()}
        </Button>
      </div>
    </form>
  </Card.Content>
</Card.Root>
