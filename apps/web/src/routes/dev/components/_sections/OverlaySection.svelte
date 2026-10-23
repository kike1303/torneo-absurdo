<script lang="ts">
  import { Button } from '#lib/components/ui/button/index.js';
  import * as Dialog from '#lib/components/ui/dialog/index.js';
  import * as DropdownMenu from '#lib/components/ui/dropdown-menu/index.js';
  import Section from './Section.svelte';

  let notifications = $state(true);
  let speed = $state('normal');
</script>

<Section title="Dialog" id="dialog">
  <Dialog.Root>
    <Dialog.Trigger>
      {#snippet child({ props })}
        <Button {...props} variant="outline" class="self-start" data-testid="dialog-trigger"
          >Open dialog</Button
        >
      {/snippet}
    </Dialog.Trigger>
    <Dialog.Content data-testid="dialog-content">
      <Dialog.Header>
        <Dialog.Title>Confirm bet</Dialog.Title>
        <Dialog.Description>You are about to bet fictional coins.</Dialog.Description>
      </Dialog.Header>
      <Dialog.Footer showCloseButton>
        <Button>Confirm</Button>
      </Dialog.Footer>
    </Dialog.Content>
  </Dialog.Root>
</Section>

<Section title="Dropdown menu" id="dropdown">
  <DropdownMenu.Root>
    <DropdownMenu.Trigger>
      {#snippet child({ props })}
        <Button {...props} variant="outline" class="self-start" data-testid="dropdown-trigger"
          >Open menu</Button
        >
      {/snippet}
    </DropdownMenu.Trigger>
    <DropdownMenu.Content class="min-w-56" data-testid="dropdown-content">
      <DropdownMenu.Label>Fighter</DropdownMenu.Label>
      <DropdownMenu.Item>View profile</DropdownMenu.Item>
      <DropdownMenu.Item disabled>Disabled item</DropdownMenu.Item>
      <DropdownMenu.Item variant="destructive">Forfeit</DropdownMenu.Item>
      <DropdownMenu.Separator />
      <DropdownMenu.CheckboxItem bind:checked={notifications}
        >Notifications</DropdownMenu.CheckboxItem
      >
      <DropdownMenu.Separator />
      <DropdownMenu.RadioGroup bind:value={speed}>
        <DropdownMenu.RadioItem value="slow">Slow</DropdownMenu.RadioItem>
        <DropdownMenu.RadioItem value="normal">Normal</DropdownMenu.RadioItem>
      </DropdownMenu.RadioGroup>
      <DropdownMenu.Separator />
      <DropdownMenu.Sub>
        <DropdownMenu.SubTrigger>More</DropdownMenu.SubTrigger>
        <DropdownMenu.SubContent>
          <DropdownMenu.Item>Share</DropdownMenu.Item>
        </DropdownMenu.SubContent>
      </DropdownMenu.Sub>
    </DropdownMenu.Content>
  </DropdownMenu.Root>
</Section>
