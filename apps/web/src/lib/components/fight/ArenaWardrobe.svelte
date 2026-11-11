<script lang="ts">
  // The 3D wardrobe of the test arena (TA-76): per side, the accessories, a skin and the colors
  // of the slots, over the same `?wear_*`, `?skin_*` and `?colors_*` of 022. Data by props; the
  // page writes the URL and remounts the stage with `onchange`.
  import XIcon from '@lucide/svelte/icons/x';
  import type { Side } from '@torneo/renderer';
  import { Button } from '#lib/components/ui/button/index.js';
  import { Label } from '#lib/components/ui/label/index.js';
  import type { ColorSlot } from '#lib/features/arena/looks.js';
  import { m } from '#lib/paraglide/messages.js';
  import type { LookSpec, LookSpecs, WardrobeCatalog } from './botarga-model.js';
  import { accessoryName, skinName } from './wardrobe-name.js';

  interface Props {
    catalog: WardrobeCatalog;
    looks: LookSpecs | undefined;
    onchange: (next: LookSpecs) => void;
    disabled?: boolean;
    idPrefix?: string;
  }

  let { catalog, looks, onchange, disabled = false, idPrefix = 'arena' }: Props = $props();

  const sides: { side: Side; label: () => string }[] = [
    { side: 'a', label: m.arena_side_a },
    { side: 'b', label: m.arena_side_b },
  ];
  const SLOT_LABELS: Record<ColorSlot, () => string> = {
    primary: m.arena_wardrobe_color_primary,
    secondary: m.arena_wardrobe_color_secondary,
    accent: m.arena_wardrobe_color_accent,
  };
  const COLOR_SLOTS = Object.keys(SLOT_LABELS) as ColorSlot[];
  const selectClass =
    'h-11 w-full rounded-lg border border-input bg-background px-3 text-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background focus-visible:outline-none disabled:opacity-50';

  function update(side: Side, change: (look: LookSpec) => LookSpec): void {
    const current = looks?.[side] ?? {};
    onchange({ ...looks, [side]: change(current) });
  }

  function toggleWear(side: Side, id: string, on: boolean): void {
    update(side, (look) => {
      const rest = (look.wear ?? []).filter((worn) => worn !== id);
      // Catalog order, so the URL does not depend on the order of the clicks.
      const wear = on ? catalog.accessories.filter((a) => a === id || rest.includes(a)) : rest;
      return { ...look, wear };
    });
  }

  function setSkin(side: Side, slug: string): void {
    update(side, (look) => ({ ...look, skin: slug === '' ? undefined : slug }));
  }

  function setColor(side: Side, slot: ColorSlot, color: string | undefined): void {
    update(side, (look) => {
      const colors = { ...look.colors };
      if (color === undefined) delete colors[slot];
      else colors[slot] = color;
      return { ...look, colors };
    });
  }
</script>

<section class="flex flex-col gap-3" data-testid="arena-wardrobe">
  <h2 class="text-base font-semibold">{m.arena_wardrobe_title()}</h2>
  <div class="grid gap-4 sm:grid-cols-2">
    {#each sides as { side, label } (side)}
      {@const look = looks?.[side]}
      <fieldset
        class="flex min-w-0 flex-col gap-3 rounded-lg border border-border p-3"
        {disabled}
        data-testid={`${idPrefix}-wardrobe-${side}`}
      >
        <legend class="px-1 text-sm font-semibold">{label()}</legend>

        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium" id={`${idPrefix}-wear-${side}-label`}>
            {m.arena_wardrobe_accessories()}
          </span>
          {#if catalog.accessories.length === 0}
            <p class="text-sm text-muted-foreground">{m.arena_wardrobe_none()}</p>
          {:else}
            <ul
              class="grid grid-cols-1 min-[420px]:grid-cols-2"
              aria-labelledby={`${idPrefix}-wear-${side}-label`}
            >
              {#each catalog.accessories as id (id)}
                <li>
                  <label
                    class="flex min-h-11 cursor-pointer items-center gap-3 rounded-md px-1 text-sm hover:bg-muted has-[:disabled]:cursor-not-allowed has-[:disabled]:opacity-50"
                  >
                    <input
                      type="checkbox"
                      class="size-5 shrink-0 accent-primary"
                      checked={look?.wear?.includes(id) ?? false}
                      onchange={(event) => toggleWear(side, id, event.currentTarget.checked)}
                      data-testid={`${idPrefix}-wear-${side}-${id}`}
                    />
                    <span class="min-w-0 break-words">{accessoryName(id)}</span>
                  </label>
                </li>
              {/each}
            </ul>
          {/if}
        </div>

        <div class="flex flex-col gap-1.5">
          <Label for={`${idPrefix}-skin-${side}`}>{m.arena_wardrobe_skin()}</Label>
          <select
            id={`${idPrefix}-skin-${side}`}
            class={selectClass}
            value={look?.skin ?? ''}
            onchange={(event) => setSkin(side, event.currentTarget.value)}
            data-testid={`${idPrefix}-skin-${side}`}
          >
            <option value="">{m.arena_wardrobe_skin_base()}</option>
            {#each catalog.skins as slug (slug)}
              <option value={slug}>{skinName(slug)}</option>
            {/each}
            {#if look?.skin !== undefined && !catalog.skins.includes(look.skin)}
              <!-- From the URL but not in content/models/: shown, so the select does not lie. -->
              <option value={look.skin}>{skinName(look.skin)}</option>
            {/if}
          </select>
        </div>

        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">{m.arena_wardrobe_colors()}</span>
          {#each COLOR_SLOTS as slot (slot)}
            {@const color = look?.colors?.[slot]}
            {@const inputId = `${idPrefix}-color-${side}-${slot}`}
            <div class="flex min-h-11 items-center gap-3">
              <input
                id={inputId}
                type="color"
                class="h-11 w-14 shrink-0 cursor-pointer rounded-md border border-input bg-background p-1 disabled:cursor-not-allowed disabled:opacity-50"
                class:border-dashed={color === undefined}
                value={color}
                onchange={(event) => setColor(side, slot, event.currentTarget.value)}
                data-testid={inputId}
              />
              <label for={inputId} class="min-w-0 flex-1 text-sm">
                {SLOT_LABELS[slot]()}
                {#if color === undefined}
                  <span class="text-muted-foreground">· {m.arena_wardrobe_color_default()}</span>
                {/if}
              </label>
              {#if color !== undefined}
                <Button
                  variant="outline"
                  size="icon"
                  class="shrink-0"
                  aria-label={m.arena_wardrobe_color_reset({ slot: SLOT_LABELS[slot]() })}
                  onclick={() => setColor(side, slot, undefined)}
                  data-testid={`${inputId}-reset`}
                >
                  <XIcon aria-hidden="true" />
                </Button>
              {/if}
            </div>
          {/each}
        </div>
      </fieldset>
    {/each}
  </div>
</section>
