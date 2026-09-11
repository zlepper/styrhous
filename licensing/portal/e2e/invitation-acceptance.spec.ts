import { expect, test, type Request } from '@playwright/test';

const invitationSecret = 'invitation-secret-that-must-not-remain-in-the-address';

test('accepts an emailed invitation and clears its secret from browser history', async (
  { page },
  testInfo
) => {
  let acceptanceRequest: Request | null = null;
  await page.route('**/api/**', async (route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    if (path === '/api/antiforgery') {
      await route.fulfill({ json: { requestToken: 'invitation-csrf-token' } });
      return;
    }
    if (path === '/api/invitations/accept' && request.method() === 'POST') {
      acceptanceRequest = request;
      await route.fulfill({
        json: {
          reasonCode: 'invitation_accepted',
          invitationId: '0191a8f0-1111-7000-8000-000000000081',
          organizationId: '0191a8f0-1111-7000-8000-000000000082',
          membershipId: '0191a8f0-1111-7000-8000-000000000083',
          seatId: '0191a8f0-1111-7000-8000-000000000084',
          productSeatAssigned: true,
          role: 'member',
          correlationId: '0191a8f0-1111-7000-8000-000000000085',
          acceptedAt: '2026-09-04T15:00:00Z'
        }
      });
      return;
    }
    await route.abort();
  });

  await page.goto(`/invitations/accept?secret=${encodeURIComponent(invitationSecret)}`);

  await expect(page.getByRole('heading', { name: 'Your organization access is ready' })).toBeVisible();
  await expect(page.getByRole('status')).toContainText('A license was assigned to you.');
  await expect(page.getByText('A license was assigned to you.')).toBeVisible();
  await expect(page).toHaveURL('/invitations/accept');
  expect(acceptanceRequest).not.toBeNull();
  expect(acceptanceRequest!.postDataJSON()).toEqual({ secret: invitationSecret });
  expect(acceptanceRequest!.headers()['x-csrf-token']).toBe('invitation-csrf-token');
  await expect.soft(page).toHaveScreenshot('invitation-accepted.png', { fullPage: true });
  expect.soft(await page.locator('body').ariaSnapshot()).toMatchSnapshot(
    'invitation-accepted-accessibility.txt'
  );

  if (testInfo.project.name === 'desktop-chromium') {
    await page.getByRole('link', { name: 'Open organizations' }).focus();
    await expect(page.getByRole('link', { name: 'Open organizations' })).toBeFocused();
  }
});

test('preserves the invitation secret across sign-in', async ({ page }) => {
  await page.route('**/api/**', async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/api/antiforgery') {
      await route.fulfill({ json: { requestToken: 'invitation-csrf-token' } });
      return;
    }
    await route.fulfill({ status: 401 });
  });
  await page.route('**/auth/providers', async (route) => {
    await route.fulfill({ json: { providers: ['github'] } });
  });

  await page.goto(`/invitations/accept?secret=${encodeURIComponent(invitationSecret)}`);

  const signIn = page.getByRole('link', { name: 'Sign in with GitHub' });
  await expect(signIn).toBeVisible();
  await expect(signIn).toHaveAttribute(
    'href',
    `/auth/sign-in/github?returnUrl=${encodeURIComponent(`/invitations/accept?secret=${invitationSecret}`)}`
  );
  await expect(page).toHaveURL(
    `/invitations/accept?secret=${encodeURIComponent(invitationSecret)}`
  );
});

test('retries acceptance after the browser session is restored', async ({ page }) => {
  let acceptanceAttempts = 0;
  await page.route('**/api/**', async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/api/antiforgery') {
      await route.fulfill({ json: { requestToken: 'invitation-csrf-token' } });
      return;
    }
    acceptanceAttempts += 1;
    if (acceptanceAttempts === 1) {
      await route.fulfill({ status: 401 });
      return;
    }
    await route.fulfill({
      json: {
        reasonCode: 'invitation_accepted',
        productSeatAssigned: false,
        role: 'admin'
      }
    });
  });
  await page.route('**/auth/providers', async (route) => {
    await route.fulfill({ json: { providers: ['github'] } });
  });

  await page.goto(`/invitations/accept?secret=${encodeURIComponent(invitationSecret)}`);
  await expect(page.getByRole('link', { name: 'Sign in with GitHub' })).toBeVisible();
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));

  await expect(page.getByRole('heading', { name: 'Your organization access is ready' })).toBeVisible();
  await expect(page.getByText('No license was assigned.')).toBeVisible();
  await expect(page).toHaveURL('/invitations/accept');
  expect(acceptanceAttempts).toBe(2);
});

test('fails a rejected invitation closed and clears its secret', async ({ page }) => {
  await page.route('**/api/**', async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/api/antiforgery') {
      await route.fulfill({ json: { requestToken: 'invitation-csrf-token' } });
      return;
    }
    await route.fulfill({ status: 404, json: { reasonCode: 'invitation_not_found' } });
  });

  await page.goto(`/invitations/accept?secret=${encodeURIComponent(invitationSecret)}`);

  await expect(page.getByRole('heading', { name: 'This invitation could not be accepted' })).toBeVisible();
  await expect(page.getByRole('alert')).toHaveText('That invitation is no longer available.');
  await expect(page).toHaveURL('/invitations/accept');
});
