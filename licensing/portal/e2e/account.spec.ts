import { expect, test, type Request } from '@playwright/test';

test('offers configured sign-in providers with a local return URL', async ({ page }) => {
  await page.route('**/auth/session', (route) =>
    route.fulfill({ json: { authenticated: false, configuredProviders: ['github', 'google'] } })
  );
  await page.route('**/auth/providers', (route) =>
    route.fulfill({ json: { providers: ['github', 'google'] } })
  );

  await page.goto('/account?source=desktop');

  await expect(page.getByRole('heading', { name: 'Sign in to manage your account' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Sign in with GitHub' })).toHaveAttribute(
    'href',
    '/auth/sign-in/github?returnUrl=%2Faccount%3Fsource%3Ddesktop'
  );
  await expect(page.getByRole('link', { name: 'Sign in with Google' })).toBeVisible();
  expect.soft(await page.locator('body').ariaSnapshot()).toMatchSnapshot(
    'account-sign-in-accessibility.txt'
  );
  await expect.soft(page).toHaveScreenshot('account-sign-in.png', { fullPage: true });
});

test('explains authentication callback failures and removes them from the next return URL', async ({
  page
}) => {
  await page.route('**/auth/session', (route) =>
    route.fulfill({ json: { authenticated: false, configuredProviders: ['github'] } })
  );
  await page.route('**/auth/providers', (route) =>
    route.fulfill({ json: { providers: ['github'] } })
  );

  await page.goto(
    '/account?source=desktop&authenticationError=verified_email_required'
  );

  await expect(page.getByRole('alert')).toHaveText(
    'Your identity provider did not supply a verified email address.'
  );
  await expect(page.getByRole('link', { name: 'Sign in with GitHub' })).toHaveAttribute(
    'href',
    '/auth/sign-in/github?returnUrl=%2Faccount%3Fsource%3Ddesktop'
  );
});

test('retries an account session that could not be loaded', async ({ page }) => {
  let sessionRequests = 0;
  await page.route('**/auth/session', (route) => {
    sessionRequests += 1;
    return sessionRequests === 1
      ? route.fulfill({ status: 503, json: { reasonCode: 'temporarily_unavailable' } })
      : route.fulfill({ json: { authenticated: false, configuredProviders: ['github'] } });
  });
  await page.route('**/auth/providers', (route) =>
    route.fulfill({ json: { providers: ['github'] } })
  );

  await page.goto('/account');
  await expect(page.getByRole('heading', { name: 'Account could not be loaded' })).toBeVisible();
  await page.getByRole('button', { name: 'Try again' }).click();

  await expect(page.getByRole('heading', { name: 'Sign in to manage your account' })).toBeVisible();
  expect(sessionRequests).toBe(2);
});

test('explains Microsoft verified-email rejection and offers other providers', async ({ page }) => {
  const configuredProviders = ['github', 'google', 'microsoft'];
  await page.route('**/auth/session', route => route.fulfill({ json: { authenticated: false, configuredProviders } }));
  await page.route('**/auth/providers', route => route.fulfill({ json: { providers: configuredProviders } }));
  await page.goto('/account?authenticationError=microsoft_verified_email_required');
  await expect(page.getByRole('alert')).toHaveText('Microsoft did not supply a verified email address. Please sign in with another provider.');
  await expect(page.getByRole('link', { name: 'Sign in with Google' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Sign in with GitHub' })).toHaveAttribute('href', '/auth/sign-in/github?returnUrl=%2Faccount');
  expect.soft(await page.locator('body').ariaSnapshot()).toMatchSnapshot('account-microsoft-email-rejected-accessibility.txt');
  await expect.soft(page).toHaveScreenshot('account-microsoft-email-rejected.png', { fullPage: true });
});

test('retries sign-in provider discovery without reloading the account page', async ({ page }) => {
  let providerRequests = 0;
  await page.route('**/auth/session', (route) =>
    route.fulfill({ json: { authenticated: false, configuredProviders: ['github'] } })
  );
  await page.route('**/auth/providers', (route) => {
    providerRequests += 1;
    return providerRequests === 1
      ? route.fulfill({ status: 503, json: { reasonCode: 'temporarily_unavailable' } })
      : route.fulfill({ json: { providers: ['github'] } });
  });

  await page.goto('/account');
  await page.getByRole('button', { name: 'Retry sign-in options' }).click();

  await expect(page.getByRole('link', { name: 'Sign in with GitHub' })).toBeVisible();
  expect(providerRequests).toBe(2);
});

test('falls back to sign-in when an account mutation finds an expired session', async ({ page }) => {
  await page.route('**/auth/session', (route) =>
    route.fulfill({
      json: {
        authenticated: true,
        userId: '01991f2a-1111-7000-8000-000000000001',
        email: 'person@example.com',
        linkedProviders: ['github', 'google'],
        configuredProviders: ['github', 'google'],
        recentlyAuthenticated: true
      }
    })
  );
  await page.route('**/auth/providers', (route) =>
    route.fulfill({ json: { providers: ['github', 'google'] } })
  );
  await page.route('**/api/antiforgery', (route) =>
    route.fulfill({ json: { requestToken: 'expired-session-token' } })
  );
  await page.route('**/auth/providers/google', (route) =>
    route.fulfill({ status: 401 })
  );

  await page.goto('/account');
  page.once('dialog', (dialog) => void dialog.accept());
  await page.getByRole('button', { name: 'Remove Google' }).click();

  await expect(page.getByRole('heading', { name: 'Sign in to manage your account' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Sign in with GitHub' })).toBeVisible();
});

test('offers reauthentication instead of account mutations for a stale session', async ({
  page
}) => {
  await page.route('**/auth/session', (route) =>
    route.fulfill({
      json: {
        authenticated: true,
        userId: '01991f2a-1111-7000-8000-000000000001',
        email: 'person@example.com',
        linkedProviders: ['github', 'google'],
        configuredProviders: ['github', 'google', 'microsoft'],
        recentlyAuthenticated: false
      }
    })
  );

  await page.goto('/account');

  await expect(page.getByRole('button', { name: /Remove/ })).toHaveCount(0);
  await expect(page.getByRole('link', { name: 'Sign in again to remove' }).first()).toHaveAttribute(
    'href',
    '/auth/reauth/github?returnUrl=/account'
  );
  await expect(
    page.getByRole('link', { name: 'Sign in again to add Microsoft' })
  ).toHaveAttribute('href', '/auth/reauth/github?returnUrl=/account');
});

test('manages linked providers and signs out through protected account mutations', async ({
  page
}) => {
  let authenticated = true;
  let providers = ['github', 'google'];
  const mutations: Request[] = [];
  let releaseUnlink = () => {};
  const unlinkGate = new Promise<void>((resolve) => {
    releaseUnlink = resolve;
  });
  await page.route('**/auth/session', (route) =>
    route.fulfill({
      json: authenticated
        ? {
            authenticated: true,
            userId: '01991f2a-1111-7000-8000-000000000001',
            email: 'person@example.com',
            linkedProviders: providers,
            configuredProviders: ['github', 'google', 'microsoft'],
            recentlyAuthenticated: true
          }
        : { authenticated: false, configuredProviders: ['github', 'google', 'microsoft'] }
    })
  );
  await page.route('**/auth/providers', (route) =>
    route.fulfill({ json: { providers: ['github', 'google', 'microsoft'] } })
  );
  await page.route('**/auth/providers/google', async (route) => {
    mutations.push(route.request());
    await unlinkGate;
    providers = providers.filter((provider) => provider !== 'google');
    await route.fulfill({ json: { reasonCode: 'provider_unlinked' } });
  });
  await page.route('**/auth/sign-out', async (route) => {
    mutations.push(route.request());
    authenticated = false;
    await route.fulfill({ status: 204 });
  });
  await page.route('**/api/antiforgery', (route) =>
    route.fulfill({ json: { requestToken: 'account-csrf-token' } })
  );

  await page.goto('/account');
  await expect(page.getByRole('heading', { level: 2, name: 'person@example.com' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Add Microsoft' })).toHaveAttribute(
    'href',
    '/auth/link/microsoft?returnUrl=/account'
  );
  expect.soft(await page.locator('body').ariaSnapshot()).toMatchSnapshot(
    'account-authenticated-accessibility.txt'
  );
  await expect.soft(page).toHaveScreenshot('account-authenticated.png', { fullPage: true });

  page.once('dialog', (dialog) => void dialog.accept());
  await page.getByRole('button', { name: 'Remove Google' }).click();
  await expect.poll(() => mutations.length).toBe(1);
  await expect(page.getByRole('button', { name: 'Sign out' })).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Remove GitHub' })).toBeDisabled();
  releaseUnlink();
  await expect(page.getByText('Google was unlinked.')).toBeVisible();
  await page.getByRole('button', { name: 'Sign out' }).click();
  await expect(page.getByRole('heading', { name: 'Sign in to manage your account' })).toBeVisible();

  expect(mutations).toHaveLength(2);
  expect(mutations.map((request) => request.method())).toEqual(['DELETE', 'POST']);
  expect(mutations.every((request) => request.headers()['x-csrf-token'] === 'account-csrf-token'))
    .toBe(true);
});
