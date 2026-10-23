<script lang="ts">
  import { Input } from '#lib/components/ui/input/index.js';
  import { Label } from '#lib/components/ui/label/index.js';
  import Section from './Section.svelte';
</script>

<Section title="Input and label" id="input">
  <div class="grid max-w-sm gap-2">
    <Label for="demo-default">Fighter name</Label>
    <Input id="demo-default" placeholder="Type a name" data-testid="input-default" />
  </div>
  <div class="grid max-w-sm gap-2">
    <Label for="demo-disabled">Disabled</Label>
    <Input id="demo-disabled" value="Read only value" disabled />
  </div>
  <div class="grid max-w-sm gap-2">
    <Label for="demo-error">With error</Label>
    <Input
      id="demo-error"
      value="Not valid"
      aria-invalid="true"
      aria-describedby="demo-error-text"
    />
    <p
      id="demo-error-text"
      class="rounded-md bg-destructive px-2 py-1 text-sm text-destructive-foreground"
    >
      This value is not valid.
    </p>
  </div>
</Section>
