import { describe, expect, it } from 'vitest';
import {
  codeFromHash,
  inviteLink,
  parseFriendName,
  parseInviteCode,
  parseTournamentForm,
} from './form.js';

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

  it('takes a tournament of 4 or 8 with size - 1 distinct names and a key', () => {
    const key = '0b0e8e5e-8a43-4c3c-9b55-0e6e3e1b2a10';
    const tournament = (size: string, names: string[], k = key) => {
      const data = new FormData();
      data.set('size', size);
      for (const name of names) data.append('display_names', name);
      data.set('client_key', k);
      return parseTournamentForm(data);
    };
    expect(tournament('4', ['Beto', ' Carla ', 'Dani'])).toEqual({
      size: 4,
      display_names: ['Beto', 'Carla', 'Dani'],
      client_key: key,
    });
    expect(tournament('8', ['a', 'b', 'c', 'd', 'e', 'f', 'g'])?.size).toBe(8);
    expect(tournament('5', ['a', 'b', 'c', 'd'])).toBeNull();
    expect(tournament('4', ['a', 'b'])).toBeNull();
    expect(tournament('4', ['a', 'a', 'b'])).toBeNull();
    expect(tournament('4', ['a', 'b', 'c'], 'nope')).toBeNull();
  });
});
