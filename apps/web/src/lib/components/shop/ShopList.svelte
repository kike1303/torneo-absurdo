<script lang="ts">
  // The items of the shop by kind (024). Each one is a link to `?item=<kind>:<id>`: picking it
  // tries it on in the preview and opens its confirmation, with or without JavaScript.
  import { Badge } from '#lib/components/ui/badge/index.js';
  import type { StatKey } from '#lib/features/botarga/catalog.js';
  import type { ShopEntry, ShopSection } from '#lib/features/shop/shop.js';
  import { formatCoins, formatInteger, formatList, formatSigned } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';
  import { labelText } from './shop-text.js';

  interface Props {
    sections: readonly ShopSection[];
    picked: { kind: string; id: string } | null;
    locale: Locale;
  }

  let { sections, picked, locale }: Props = $props();

  const TITLE = {
    cosmetic: m.shop_section_cosmetic,
    accessory: m.shop_section_accessory,
    skin: m.shop_section_skin,
  } as const;

  const STAT_LABEL: Record<StatKey, () => string> = {
    hp: m.botarga_stat_hp,
    attack: m.botarga_stat_attack,
    defense: m.botarga_stat_defense,
    speed: m.botarga_stat_speed,
    special: m.botarga_stat_special,
  };

  function deltasText(entry: ShopEntry): string {
    return formatList(
      entry.deltas.map(({ stat, delta }) =>
        m.botarga_wear_delta({ delta: formatSigned(delta, locale), stat: STAT_LABEL[stat]() }),
      ),
      locale,
    );
  }

  function badge(entry: ShopEntry): string | null {
    if (entry.state === 'owned') return m.shop_state_owned();
    if (entry.state === 'in_draw') return m.shop_state_in_draw();
    if (entry.state === 'level' && entry.level !== null)
      return m.shop_state_level({ level: formatInteger(entry.level, locale) });
    return null;
  }
</script>

<div class="flex flex-col gap-6" data-testid="shop-list">
  {#each sections as section (section.kind)}
    <section class="flex flex-col gap-2" aria-labelledby={`shop-${section.kind}`}>
      <h2 id={`shop-${section.kind}`} class="font-display text-xl font-semibold">
        {TITLE[section.kind]()}
      </h2>
      <ul class="flex flex-col gap-2">
        {#each section.entries as entry (entry.id)}
          {@const on = picked?.kind === entry.kind && picked.id === entry.id}
          {@const note = badge(entry)}
          <li>
            <a
              href={`?item=${entry.param}`}
              data-sveltekit-replacestate
              aria-current={on ? 'true' : undefined}
              class="flex min-h-11 items-center gap-3 rounded-lg border border-border px-3 py-2 transition-colors hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none aria-[current=true]:border-primary aria-[current=true]:bg-accent"
              data-testid="shop-item"
              data-item={entry.param}
              data-state={entry.state}
            >
              <span class="flex min-w-0 flex-1 flex-col">
                <span class="truncate font-medium">{labelText(entry.label, locale)}</span>
                {#if entry.deltas.length > 0}
                  <span class="text-sm text-muted-foreground">{deltasText(entry)}</span>
                {/if}
              </span>
              {#if note !== null}
                <Badge variant="outline">{note}</Badge>
              {:else}
                <span class="text-sm font-medium whitespace-nowrap tabular-nums">
                  {m.shop_price({ amount: formatCoins(entry.price, locale) })}
                </span>
              {/if}
            </a>
          </li>
        {/each}
      </ul>
    </section>
  {/each}
</div>
