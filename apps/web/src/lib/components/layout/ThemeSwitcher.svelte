<script lang="ts">
  import MonitorIcon from '@lucide/svelte/icons/monitor';
  import MoonIcon from '@lucide/svelte/icons/moon';
  import SunIcon from '@lucide/svelte/icons/sun';
  import { onMount } from 'svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import * as DropdownMenu from '#lib/components/ui/dropdown-menu/index.js';
  import { m } from '#lib/paraglide/messages.js';
  import { THEME_CHOICES, type ThemeChoice } from '#lib/theme/resolve-theme.js';
  import { setTheme, theme } from '#lib/theme/theme.svelte.js';

  // `onSave` (with a session, 008 RF-D4): also saves the choice in the profile; `false` shows a
  // message in a live region, and the theme stays applied with the cookie on this device.
  let {
    initial,
    onSave,
  }: { initial: ThemeChoice; onSave?: (choice: ThemeChoice) => Promise<boolean> } = $props();

  let saveFailed = $state(false);
  async function choose(choice: ThemeChoice): Promise<void> {
    setTheme(choice);
    if (!onSave) return;
    saveFailed = !(await onSave(choice));
  }

  // The runa is module state: on the server it would be shared between requests, so the SSR
  // label comes from the `initial` prop and the runa takes over once mounted.
  let mounted = $state(false);
  onMount(() => {
    mounted = true;
  });
  const current = $derived(mounted ? theme.choice : initial);

  const names: Record<ThemeChoice, () => string> = {
    dark: () => m.theme_name_dark(),
    light: () => m.theme_name_light(),
    system: () => m.theme_name_system(),
  };
  const icons = { dark: MoonIcon, light: SunIcon, system: MonitorIcon } as const;
  const CurrentIcon = $derived(icons[current]);
</script>

<DropdownMenu.Root>
  <DropdownMenu.Trigger>
    {#snippet child({ props })}
      <Button
        {...props}
        variant="outline"
        aria-label={m.theme_switcher_current({ name: names[current]() })}
        data-testid="theme-switcher"
      >
        <CurrentIcon aria-hidden="true" />
        <span>{names[current]()}</span>
      </Button>
    {/snippet}
  </DropdownMenu.Trigger>
  <DropdownMenu.Content align="end" class="min-w-44">
    <DropdownMenu.Label>{m.theme_switcher_label()}</DropdownMenu.Label>
    {#each THEME_CHOICES as choice (choice)}
      <DropdownMenu.CheckboxItem
        checked={current === choice}
        closeOnSelect
        onCheckedChange={() => choose(choice)}
        data-testid={`theme-option-${choice}`}
      >
        {names[choice]()}
      </DropdownMenu.CheckboxItem>
    {/each}
  </DropdownMenu.Content>
</DropdownMenu.Root>
<p
  class="basis-full text-end text-sm text-muted-foreground empty:hidden"
  aria-live="polite"
  data-testid="theme-save-status"
>
  {#if saveFailed}{m.preferences_save_failed()}{/if}
</p>
