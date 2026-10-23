<script lang="ts">
  import '../app.css';
  import { Button } from '#lib/components/ui/button/index.js';
  import { m } from '#lib/paraglide/messages.js';
</script>

<main>
  <h1>{m.home_title()}</h1>
  <Button>{m.home_coming_soon()}</Button>
</main>
