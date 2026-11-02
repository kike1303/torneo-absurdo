// The id of a fight in a URL: the canonical decimal of 010 RF34, from 1 to `i32::MAX`, with no sign,
// no zeros on the left and no spaces (011 RF17, RF18). Kit decodes the path, and `handleFetch` adds
// the internal secret, the IP and the cookie to what the SSR asks for: only what passes here ever
// reaches a read, and it does so as a number.

const MAX_ID = 2_147_483_647;
const CANONICAL = /^[1-9][0-9]*$/;

export function parseFightId(text: string): number | null {
  if (!CANONICAL.test(text)) return null;
  const id = Number(text);
  return id <= MAX_ID ? id : null;
}
