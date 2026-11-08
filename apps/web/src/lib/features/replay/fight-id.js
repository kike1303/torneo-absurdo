// The id of a fight in a URL: the canonical decimal of 010 RF34, from 1 to `i32::MAX`, with no sign,
// no zeros on the left and no spaces (011 RF17, RF18). Kit decodes the path, and `handleFetch` adds
// the internal secret, the IP and the cookie to what the SSR asks for: only what passes here ever
// reaches a read, and it does so as a number.
//
// JavaScript with JSDoc, not TypeScript: `vite build` imports src/params.ts with plain Node to check
// the matchers, and there the path of this import has to exist as it is written.

const MAX_ID = 2_147_483_647;
const CANONICAL = /^[1-9][0-9]*$/;

/**
 * @param {string} text
 * @returns {number | null}
 */
export function parseFightId(text) {
  if (!CANONICAL.test(text)) return null;
  const id = Number(text);
  return id <= MAX_ID ? id : null;
}
