import type { PageServerLoad } from './$types';

// RF-A20: throws an unexpected error whose message carries a URL with a query and a canary IP.
// Only reachable behind the /dev/** guard. The e2e of A19 checks none of it reaches the logs or
// the page.
export const load: PageServerLoad = () => {
  throw new Error(
    'error-probe failure fetching http://canary.example/api/x?token=canary-query-7d1e from 203.0.113.99',
  );
};
