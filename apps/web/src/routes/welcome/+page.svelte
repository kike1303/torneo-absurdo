<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';
</script>

<!-- Never shown: the load always redirects. -->
<p><a class="underline underline-offset-4" href={`/${getLocale()}/`}>{m.error_home_link()}</a></p>
