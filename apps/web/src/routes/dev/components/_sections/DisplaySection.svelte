<script lang="ts">
  import { Badge, type BadgeVariant } from '#lib/components/ui/badge/index.js';
  import { Button } from '#lib/components/ui/button/index.js';
  import * as Card from '#lib/components/ui/card/index.js';
  import { Separator } from '#lib/components/ui/separator/index.js';
  import { Skeleton } from '#lib/components/ui/skeleton/index.js';
  import Section from './Section.svelte';

  const badges: BadgeVariant[] = [
    'default',
    'secondary',
    'outline',
    'ghost',
    'destructive',
    'link',
  ];
</script>

<Section title="Badge" id="badge">
  <div class="flex flex-wrap items-center gap-2">
    {#each badges as variant (variant)}
      <Badge {variant}>{variant}</Badge>
    {/each}
  </div>
</Section>

<Section title="Card" id="card">
  <Card.Root class="max-w-sm">
    <Card.Header>
      <Card.Title>Next fight</Card.Title>
      <Card.Description>Two tribute fighters, one arena.</Card.Description>
    </Card.Header>
    <Card.Content>
      <p>Card content with regular text.</p>
    </Card.Content>
    <Card.Footer class="gap-2">
      <Button variant="outline">Details</Button>
      <Button>Bet</Button>
    </Card.Footer>
  </Card.Root>
</Section>

<Section title="Separator and skeleton" id="skeleton">
  <div class="flex h-6 items-center gap-3">
    <span>Left</span>
    <Separator orientation="vertical" />
    <span>Right</span>
  </div>
  <div class="flex items-center gap-3" aria-hidden="true">
    <Skeleton class="size-11 rounded-full" />
    <div class="flex flex-col gap-2">
      <Skeleton class="h-4 w-48" />
      <Skeleton class="h-4 w-32" />
    </div>
  </div>
</Section>
