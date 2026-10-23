<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import { cn } from "#lib/utils.js";

  let count = $state(1);
  const format = (n: number, locale: string) => new Intl.NumberFormat(locale).format(n);
</script>

{#if count === 1 && m.hello() !== "x"}
  <p class={cn("text-foreground", count > 1 ? "font-bold" : "")}>{count === 1 ? m.hello() : m.bye()}</p>
{/if}
<span aria-hidden="true">{"·"}</span>
<span>{`${count} / ${count + 1}`}</span>
<span title={m.hello()}>{format(count, "en")}</span>
<a href={"/es/x"} aria-label={m.bye()}>{count}</a>
