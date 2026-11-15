import { describe, expect, it } from 'vitest';
import { codeFromHash, inviteLink, parseFriendName, parseInviteCode } from './form.js';

const CODE = 'AbCdEfGhIjKlMnOpQr_-12';

function form(fields: Record<string, string>): FormData {
  const data = new FormData();
  for (const [key, value] of Object.entries(fields)) data.set(key, value);
  return data;
}

describe('friends forms', () => {
  it('takes a code only with its form', () => {
    expect(parseInviteCode(form({ code: CODE }))).toBe(CODE);
    for (const bad of ['', 'short', `${CODE}x`, 'AbCdEfGhIjKlMnOpQr/=12']) {
      expect(parseInviteCode(form({ code: bad }))).toBeNull();
    }
    expect(parseInviteCode(form({}))).toBeNull();
  });

  it('reads the code of the fragment', () => {
    expect(codeFromHash(`#${CODE}`)).toBe(CODE);
    expect(codeFromHash(CODE)).toBe(CODE);
    expect(codeFromHash('')).toBeNull();
    expect(codeFromHash('#nope')).toBeNull();
  });

  it('puts the code in the fragment of the link', () => {
    expect(inviteLink('https://example.test', 'es', CODE)).toBe(
      `https://example.test/es/friends/invite#${CODE}`,
    );
  });

  it('trims a name and refuses an empty or huge one', () => {
    expect(parseFriendName(form({ display_name: '  Ana  ' }))).toBe('Ana');
    expect(parseFriendName(form({ display_name: '   ' }))).toBeNull();
    expect(parseFriendName(form({ display_name: 'x'.repeat(300) }))).toBeNull();
    expect(parseFriendName(form({}))).toBeNull();
  });
});
