<script lang="ts">
  import '../app.css';
  import { Button } from '#lib/components/ui/button/index.js';
  import { probeClass } from '#lib/probe/classes.js';

  let { data } = $props();
</script>

<main class={probeClass}>
  <Button data-catalog={data.catalog}>{'probe'}</Button>
</main>
