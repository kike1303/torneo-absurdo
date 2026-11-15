// The fields of the friends forms (023), checked at the edge before any call. The server checks
// them again; these only keep out what no friend or code can be.

/** An invite code: 22 characters of base64url, as the server makes them. */
export const INVITE_CODE = /^[A-Za-z0-9_-]{22}$/;

/** The longest name a form may send; the server refuses longer ones anyway. */
const NAME_MAX = 256;

/** The `display_name` of a friend form, or null for one no friend can have. */
export function parseFriendName(form: FormData): string | null {
  const value = form.get('display_name');
  if (typeof value !== 'string') return null;
  const name = value.trim();
  return name.length > 0 && name.length <= NAME_MAX ? name : null;
}

/** The `code` of an invite form, or null if it has not the form of a code. */
export function parseInviteCode(form: FormData): string | null {
  const value = form.get('code');
  return typeof value === 'string' && INVITE_CODE.test(value) ? value : null;
}

/** The code in the fragment of an invite link (`#<code>`), or null. */
export function codeFromHash(hash: string): string | null {
  const code = hash.startsWith('#') ? hash.slice(1) : hash;
  return INVITE_CODE.test(code) ? code : null;
}

/** The invite link of `code`: the code goes in the fragment, which never reaches a server. */
export function inviteLink(origin: string, locale: string, code: string): string {
  return `${origin}/${locale}/friends/invite#${code}`;
}
