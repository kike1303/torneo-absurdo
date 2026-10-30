// The ports and the network guard of the e2e (008 plan-d decision 10), shared by
// playwright.config.ts and the specs that open their own browser contexts.
export const LOOPBACK = '127.0.0.1';
export const EDGE_PORT = 4173;
export const NO_CATALOG_PORT = 4174;
export const CATALOG_PORT = 4175;
/** The web of e2e/internal-fetch.spec.ts, against its own recording API (never the server). */
export const INTERNAL_FETCH_WEB_PORT = 4176;
export const SERVER_PORT = 4180;
export const EGRESS_PORT = 4190;
/** The origin the browser sees: the edge's (RF-D7). */
export const PUBLIC_ORIGIN = `http://localhost:${EDGE_PORT}`;
/** What is not loopback goes to the egress proxy, which records it and cuts it. */
export const EGRESS_PROXY = {
  server: `http://${LOOPBACK}:${EGRESS_PORT}`,
  bypass: 'localhost,127.0.0.1,::1',
};
