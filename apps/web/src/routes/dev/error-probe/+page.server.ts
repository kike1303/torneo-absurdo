import type { PageServerLoad } from './$types';

// RF-A20 and ADR-0012 A5: throws an unexpected error whose message carries a URL with a query, a
// canary IP and a canary text, and whose name carries a canary with a character outside the
// identifier regex. Only reachable behind the /dev/** guard. The e2e of A19 checks none of it
// reaches the logs or the page: the message is not in the stack and the name is logged as `other`.
export const load: PageServerLoad = () => {
  const error = new Error(
    'error-probe failure fetching http://canary.example/api/x?token=canary-query-7d1e from 203.0.113.99 canary-message-0a9f',
  );
  error.name = 'canary-name-b41e';
  throw error;
};
