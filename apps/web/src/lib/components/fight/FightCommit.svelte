<script lang="ts">
  // The commit of the seed (ADR-0002, 010 E4-12): closed, whole and copyable. An attribute cannot be
  // read on a phone.
  import { m } from '#lib/paraglide/messages.js';

  let { commit }: { commit: string } = $props();
</script>

<details class="text-sm" data-testid="fight-commit">
  <summary
    class="flex min-h-11 cursor-pointer items-center rounded-lg text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background focus-visible:outline-none"
  >
    {m.fight_view_commit()}
  </summary>
  <p class="text-muted-foreground">{m.fight_view_commit_help()}</p>
  <code class="mt-1 block font-mono break-all select-all" data-testid="fight-commit-value"
    >{commit}</code
  >
</details>
