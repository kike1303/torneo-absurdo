<script lang="ts">
  // A notice of the view (010 RF54, RF57): in a `status` region, without repeating itself.
  let { text, testid }: { text: string; testid: string } = $props();
</script>

<p
  role="status"
  class="rounded-lg border border-border bg-muted p-3 text-sm text-foreground"
  data-testid={testid}
>
  {text}
</p>
