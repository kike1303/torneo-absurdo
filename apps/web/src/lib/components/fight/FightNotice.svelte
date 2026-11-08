<script lang="ts">
  // A notice of the view (010 RF54, RF57): in a `status` region, without repeating itself.
  interface Props {
    text: string;
    testid: string;
    /** Somewhere to go meanwhile (011 RF26). */
    link?: { readonly href: string; readonly text: string; readonly testid: string };
  }

  let { text, testid, link }: Props = $props();
</script>

<p
  role="status"
  class="rounded-lg border border-border bg-muted p-3 text-sm text-foreground"
  data-testid={testid}
>
  {text}
  {#if link !== undefined}
    <a
      href={link.href}
      class="inline-flex min-h-11 items-center underline underline-offset-4 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
      data-testid={link.testid}
    >
      {link.text}
    </a>
  {/if}
</p>
