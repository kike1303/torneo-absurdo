import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import { m } from '#lib/paraglide/messages.js';
import LoginPage from './+page.svelte';

const page = (over: { signedIn: boolean; errorKey: 'error_auth_reauth_required' | null }) =>
  render(LoginPage, {
    props: { data: { providers: ['dev'], next: '/en/welcome', ...over } },
  } as never).body;

// TA-31 (009 amendment of RF24): with a session there is nothing to sign in to.
describe('/login', () => {
  it('after a failed confirmation, with a session, it does not ask to sign in', () => {
    const body = page({ signedIn: true, errorKey: 'error_auth_reauth_required' });
    expect(body).toContain(m.login_signed_in_title());
    expect(body).toContain(m.login_signed_in_error_title());
    expect(body).toContain(m.error_auth_reauth_required());
    expect(body).toContain(m.login_back_to_account());
    expect(body).not.toContain(m.login_lead());
    expect(body).not.toContain(m.login_error_title());
    expect(body).not.toContain(`>${m.login_title()}<`);
  });

  it('with a session and no error, it does not ask to sign in either', () => {
    const body = page({ signedIn: true, errorKey: null });
    expect(body).toContain(m.login_signed_in_title());
    expect(body).not.toContain(m.login_lead());
  });

  it('without a session, the sign-in texts stay, the error included', () => {
    const body = page({ signedIn: false, errorKey: 'error_auth_reauth_required' });
    expect(body).toContain(m.login_title());
    expect(body).toContain(m.login_lead());
    expect(body).toContain(m.login_error_title());
    expect(body).not.toContain(m.login_signed_in_title());
  });
});
