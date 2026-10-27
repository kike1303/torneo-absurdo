<script lang="ts">
  // Frame statistics of the test arena (RF-A19), with "copy JSON" for the measurement of the
  // PR. Only `import type` from the renderer: this component is in the catalog (ADR-0009 D4).
  import type { FrameStats } from '@torneo/renderer';
  import { Button } from '#lib/components/ui/button/index.js';
  import { formatDecimal, formatInteger } from '#lib/i18n/format.js';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';

  interface Props {
    stats: FrameStats;
    /** Context copied with the stats: no user agent (002 plan-a2 decision 12). */
    context: { devicePixelRatio: number; canvasWidth: number; fighters: readonly string[] };
  }

  let { stats, context }: Props = $props();
  let copied = $state(false);

  const locale = $derived(getLocale());
  const rows = $derived([
    { key: 'frames', label: m.arena_stats_frames(), value: formatInteger(stats.frames, locale) },
    { key: 'fps', label: m.arena_stats_fps_mean(), value: formatDecimal(stats.fpsMean, locale, 1) },
    { key: 'p50', label: m.arena_stats_p50(), value: formatDecimal(stats.p50, locale, 2) },
    { key: 'p95', label: m.arena_stats_p95(), value: formatDecimal(stats.p95, locale, 2) },
    { key: 'p99', label: m.arena_stats_p99(), value: formatDecimal(stats.p99, locale, 2) },
    { key: 'over33', label: m.arena_stats_over_33(), value: formatInteger(stats.over33, locale) },
  ]);

  /** "Copied" stays this long, whatever the stats refresh does (QA F3). */
  const COPIED_MS = 2000;
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function copy(): Promise<void> {
    const json = JSON.stringify({ ...stats, ...context }, null, 2);
    clearTimeout(timer);
    try {
      await navigator.clipboard.writeText(json);
      copied = true;
      timer = setTimeout(() => (copied = false), COPIED_MS);
    } catch {
      copied = false;
    }
  }

  $effect(() => () => clearTimeout(timer));
</script>

<section class="flex flex-col gap-3" aria-labelledby="arena-stats-title" data-testid="arena-stats">
  <h2 id="arena-stats-title" class="font-display text-lg font-semibold">{m.arena_stats_title()}</h2>
  <dl class="grid grid-cols-2 gap-x-4 gap-y-2 text-sm sm:grid-cols-3">
    {#each rows as row (row.key)}
      <div class="flex flex-col">
        <dt class="text-muted-foreground">{row.label}</dt>
        <dd class="font-mono tabular-nums" data-testid={`arena-stats-${row.key}`}>{row.value}</dd>
      </div>
    {/each}
  </dl>
  <div class="flex items-center gap-3">
    <Button variant="outline" onclick={copy} data-testid="arena-copy-stats"
      >{m.arena_copy_stats()}</Button
    >
    <p aria-live="polite" class="text-sm text-muted-foreground">{copied ? m.arena_copied() : ''}</p>
  </div>
</section>
