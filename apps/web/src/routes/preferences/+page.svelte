<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';
</script>

<!-- Never shown: the load redirects every GET to the account. -->
<p>
  <a class="underline underline-offset-4" href={`/${getLocale()}/account`}>{m.account_link()}</a>
</p>
