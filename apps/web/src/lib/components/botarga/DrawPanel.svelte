<script lang="ts">
  // The open draw (016 "Sorteo de ataques", 021): its options as cards, one form each. The page
  // shows it before the rest, and the build waits until one is chosen. The options are the server's
  // and never change; the queue says how many levels wait their turn.
  import { enhance } from '$app/forms';
  import type { PendingDraw } from '@torneo/protocol';
  import { Button } from '#lib/components/ui/button/index.js';
  import * as Card from '#lib/components/ui/card/index.js';
  import type { DrawOption } from '#lib/features/botarga/view.js';
  import { formatInteger } from '#lib/i18n/format.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';

  interface Props {
    draw: PendingDraw;
    options: readonly DrawOption[];
    queued: number;
    locale: Locale;
  }

  let { draw, options, queued, locale }: Props = $props();
  let pending = $state(false);

  function label(option: DrawOption): string {
    if (option.kind === 'ability') return option.name;
    const n = formatInteger(option.clip.n, locale);
    return option.clip.clip === 'win' ? m.botarga_clip_victory({ n }) : m.botarga_clip_idle({ n });
  }
</script>

<section
  class="flex flex-col gap-3 rounded-xl border-2 border-primary p-4"
  aria-labelledby="draw-title"
  data-testid="botarga-draw"
  data-kind={draw.kind}
>
  <h2 id="draw-title" class="font-display text-xl font-semibold">
    {m.botarga_draw_title({ level: formatInteger(draw.level, locale) })}
  </h2>
  <p class="text-muted-foreground">
    {draw.kind === 'ability' ? m.botarga_draw_lead() : m.botarga_draw_lead_cosmetic()}
  </p>
  <ul class="grid gap-3 sm:grid-cols-3">
    {#each options as option (option.id)}
      <li>
        <Card.Root class="h-full">
          <Card.Content class="flex h-full flex-col justify-between gap-3">
            <p class="font-semibold" id={`draw-option-${option.id}`}>{label(option)}</p>
            <form
              method="post"
              action="?/choose"
              use:enhance={() => {
                pending = true;
                return async ({ update }) => {
                  await update({ reset: false });
                  pending = false;
                };
              }}
            >
              <input type="hidden" name="draw_id" value={draw.id} />
              <input type="hidden" name="item_id" value={option.id} />
              <Button
                type="submit"
                class="min-h-11 w-full"
                disabled={pending}
                aria-describedby={`draw-option-${option.id}`}
                data-testid="botarga-draw-option"
              >
                {m.botarga_draw_choose()}
              </Button>
            </form>
          </Card.Content>
        </Card.Root>
      </li>
    {/each}
  </ul>
  {#if queued > 0}
    <p class="text-sm text-muted-foreground" data-testid="botarga-draw-queued">
      {m.botarga_draw_queued({ n: formatInteger(queued, locale) })}
    </p>
  {/if}
</section>
