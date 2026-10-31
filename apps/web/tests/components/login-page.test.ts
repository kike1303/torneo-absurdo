import { render } from 'svelte/server';
import { beforeAll, describe, expect, it } from 'vitest';
import { resolveCallbackError } from '../../src/lib/features/auth/error-text.js';
import { overwriteGetLocale } from '../../src/lib/paraglide/runtime.js';
import LoginPage from '../../src/routes/login/+page.svelte';
import es from '../../messages/es.json' with { type: 'json' };

// 009 RF24: /login with a session, and the way back to the account.
beforeAll(() => overwriteGetLocale(() => 'es'));

const escaped = (text: string) => text.replaceAll('&', '&amp;').replaceAll('<', '&lt;');

function rendered(error: string | null, signedIn: boolean) {
  return render(LoginPage, {
    props: {
      data: {
        errorKey: resolveCallbackError(error),
        signedIn,
        providers: ['dev'],
        next: '/es/welcome',
      },
    } as never,
  });
}

function html(error: string | null, signedIn: boolean): string {
  return rendered(error, signedIn).body;
}

/** The opening tag of the element with a `data-testid`. */
function openingTag(markup: string, testid: string): string {
  const at = markup.indexOf(`data-testid="${testid}"`);
  if (at < 0) throw new Error(`no ${testid}`);
  return markup.slice(markup.lastIndexOf('<', at), markup.indexOf('>', at) + 1);
}

const PRIMARY = 'bg-primary';

describe('/login with a session and auth.reauth_mismatch', () => {
  const page = () => html('auth.reauth_mismatch', true);

  it('shows the text of RF24', () => {
    expect(page()).toContain(escaped(es.error_auth_reauth_mismatch));
  });

  it('the way back is the main action, to /es/account', () => {
    const link = openingTag(page(), 'login-account-link');
    expect(link).toContain('href="/es/account"');
    expect(link).toContain(PRIMARY);
    expect(link).toContain('w-full');
    expect(page()).toContain(escaped(es.login_back_to_account));
    expect(page()).not.toContain(`>${escaped(es.account_link)}<`);
  });

  it('auth.reauth_required behaves the same', () => {
    const required = html('auth.reauth_required', true);
    expect(required).toContain(escaped(es.error_auth_reauth_required));
    expect(required).toContain(escaped(es.login_back_to_account));
  });
});

describe('/login with a session and no error', () => {
  it('keeps the outline link to the account', () => {
    const page = html(null, true);
    const link = openingTag(page, 'login-account-link');
    expect(link).toContain('href="/es/account"');
    expect(link).not.toContain(PRIMARY);
    expect(page).toContain(escaped(es.account_link));
    expect(page).not.toContain(escaped(es.login_back_to_account));
  });
});

describe('/login without a session', () => {
  it('has no way back to the account, with or without an error', () => {
    for (const error of [null, 'auth.reauth_mismatch']) {
      const page = html(error, false);
      expect(page).not.toContain('login-account-link');
      expect(page).toContain('login-provider-dev');
    }
  });
});

// TA-31 (009 amendment of RF24): with a session there is nothing to sign in to.
describe('/login, its texts', () => {
  it('after a failed confirmation, with a session, it does not ask to sign in', () => {
    const page = html('auth.reauth_required', true);
    expect(page).toContain(escaped(es.login_signed_in_title));
    expect(page).toContain(escaped(es.login_signed_in_error_title));
    expect(page).not.toContain(escaped(es.login_lead));
    expect(page).not.toContain(escaped(es.login_error_title));
    expect(page).not.toContain(`>${escaped(es.login_title)}<`);
  });

  it('with a session and no error, it does not ask to sign in either', () => {
    const page = html(null, true);
    expect(page).toContain(escaped(es.login_signed_in_title));
    expect(page).not.toContain(escaped(es.login_lead));
  });

  it('without a session, the sign-in texts stay, the error included', () => {
    const page = html('auth.reauth_required', false);
    expect(page).toContain(escaped(es.login_title));
    expect(page).toContain(escaped(es.login_lead));
    expect(page).toContain(escaped(es.login_error_title));
    expect(page).not.toContain(escaped(es.login_signed_in_title));
  });

  it('the <title> follows the session', () => {
    expect(rendered(null, true).head).toContain(`<title>${escaped(es.login_signed_in_title)} `);
    expect(rendered(null, false).head).toContain(`<title>${escaped(es.login_title)} `);
  });
});
