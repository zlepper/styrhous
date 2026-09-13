import { expect, test, type Page, type Request } from '@playwright/test';

const userCode = 'ABCD-EFGH';
const personalSeatId = '0191a8f0-1111-7000-8000-000000000081';
const organizationSeatId = '0191a8f0-1111-7000-8000-000000000082';

test('reviews capacity, revokes a device, and approves the selected seat', async ({ page }) => {
  const state = await installDeviceAuthorizationApi(page);

  await page.goto(`/devices/authorize?user_code=${userCode}`);
  await expect(page).toHaveTitle('Authorize device · Styrhous');
  await expect(
    page.getByRole('heading', { level: 1, name: 'Authorize device' })
  ).toBeVisible();
  await expect(page.getByRole('heading', { level: 2, name: 'Rasmus workstation' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Approve sign-in' })).toBeDisabled();

  await page.getByRole('radio', { name: /Northstar Platform/ }).check();
  await expect(page.getByRole('heading', { name: 'Make room before approving' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Approve sign-in' })).toBeDisabled();

  await page.evaluate(() => {
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
    window.scrollTo(0, 0);
  });

  const approveBounds = await page.getByRole('button', { name: 'Approve sign-in' }).boundingBox();
  const denyBounds = await page.getByRole('button', { name: 'Deny sign-in' }).boundingBox();
  expect(approveBounds).not.toBeNull();
  expect(denyBounds).not.toBeNull();
  expect(denyBounds!.y).toBeCloseTo(approveBounds!.y, 0);
  expect(denyBounds!.height).toBeCloseTo(approveBounds!.height, 0);

  expect.soft(await page.locator('body').ariaSnapshot()).toMatchSnapshot(
    'device-authorization-accessibility.txt'
  );
  await expect.soft(page).toHaveScreenshot('device-authorization.png', { fullPage: true });

  page.once('dialog', (dialog) => void dialog.accept());
  await page.getByRole('button', { name: 'Remove device Build host' }).click();
  await expect(page.getByRole('heading', { name: 'Make room before approving' })).toHaveCount(0);
  await expect(page.getByText('2 of 3 devices active')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Approve sign-in' })).toBeEnabled();

  await page.getByRole('button', { name: 'Approve sign-in' }).click();
  await expect(page.getByText(/Rasmus workstation is approved/)).toBeFocused();
  await expect(page.getByText('Waiting for approval', { exact: true })).toHaveCount(0);

  expect(state.approvalRequests).toHaveLength(1);
  expect(state.approvalRequests[0].postData()).toBe(
    `user_code=${userCode}&decision=approve&seat_id=${organizationSeatId}`
  );
  expect(state.approvalRequests[0].headers()['x-csrf-token']).toBe('approval-csrf-token');
  expect(state.revocationRequests).toHaveLength(1);
});

test('approves the sole license without asking for a selection', async ({ page }) => {
  const state = await installDeviceAuthorizationApi(page, true);
  await page.goto(`/devices/authorize?user_code=${userCode}`);
  await expect(page.getByText('Personal seat', { exact: true })).toBeVisible();
  await expect(page.getByRole('radio')).toHaveCount(0);
  await page.getByRole('button', { name: 'Approve sign-in' }).click();
  await expect(page.getByText(/Rasmus workstation is approved/)).toBeFocused();
  await expect(page.getByText('Waiting for approval', { exact: true })).toHaveCount(0);
  expect(state.approvalRequests[0].postData()).toContain(`seat_id=${personalSeatId}`);
});

test('denies the pending desktop without requiring a seat selection', async ({ page }) => {
  const state = await installDeviceAuthorizationApi(page);

  await page.goto(`/devices/authorize?user_code=${userCode}`);
  await page.getByRole('button', { name: 'Deny sign-in' }).click();

  await expect(page.getByText(/authorization was denied/)).toBeFocused();
  await expect(page.getByText('Waiting for approval', { exact: true })).toHaveCount(0);
  expect(state.approvalRequests).toHaveLength(1);
  expect(state.approvalRequests[0].postData()).toBe(
    `user_code=${userCode}&decision=deny`
  );
});

test('explains a missing user code without calling the backend', async ({ page }) => {
  const state = await installDeviceAuthorizationApi(page);

  await page.goto('/devices/authorize');

  await expect(
    page.getByRole('heading', { name: 'This desktop request cannot be approved' })
  ).toBeVisible();
  await expect(page.getByText(/does not contain a user code/)).toBeVisible();
  expect(state.approvalReads).toBe(0);
});

test('recovers after the user signs in and retries the pending code', async ({ page }) => {
  const state = await installDeviceAuthorizationApi(page);
  let requireAuthentication = true;
  await page.route('**/desktop/v1/device/approval**', async (route) => {
    if (route.request().method() === 'GET' && requireAuthentication) {
      requireAuthentication = false;
      await route.fulfill({ status: 401 });
      return;
    }

    await route.fallback();
  });

  await page.goto(`/devices/authorize?user_code=${userCode}`);
  await expect(page.getByRole('heading', { name: 'Sign in before approving this desktop' }))
    .toBeVisible();
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));

  await expect(page.getByRole('heading', { level: 2, name: 'Rasmus workstation' })).toBeVisible();
  expect(state.approvalReads).toBe(1);
});

test('clears authentication state when navigation removes the user code', async ({ page }) => {
  await installDeviceAuthorizationApi(page);
  await page.route('**/desktop/v1/device/approval**', async (route) => {
    if (route.request().method() === 'GET') {
      await route.fulfill({ status: 401 });
      return;
    }

    await route.fallback();
  });

  await page.goto(`/devices/authorize?user_code=${userCode}`);
  await expect(page.getByRole('heading', { name: 'Sign in before approving this desktop' }))
    .toBeVisible();

  await page.evaluate(() => {
    window.history.pushState({}, '', '/devices/authorize');
    window.dispatchEvent(new PopStateEvent('popstate'));
  });

  await expect(
    page.getByRole('heading', { name: 'This desktop request cannot be approved' })
  ).toBeVisible();
  await expect(page.getByText(/does not contain a user code/)).toBeVisible();
  await expect(
    page.getByRole('heading', { name: 'Sign in before approving this desktop' })
  ).toHaveCount(0);
});

test('shows an expired-code state without exposing an approval form', async ({ page }) => {
  await installDeviceAuthorizationApi(page);
  await page.route('**/desktop/v1/device/approval**', async (route) => {
    if (route.request().method() === 'GET') {
      await route.fulfill({
        status: 404,
        json: { reasonCode: 'device_authorization_not_found' }
      });
      return;
    }

    await route.fallback();
  });

  await page.goto(`/devices/authorize?user_code=${userCode}`);

  await expect(
    page.getByRole('heading', { name: 'This desktop request cannot be approved' })
  ).toBeVisible();
  await expect(page.getByText(/invalid or has expired/)).toBeVisible();
  await expect(page.getByRole('button', { name: 'Approve sign-in' })).toHaveCount(0);
});

test('reloads the authoritative approval after a decision conflict', async ({ page }) => {
  const state = await installDeviceAuthorizationApi(page);
  let rejectDecision = true;
  await page.route('**/desktop/v1/device/approval', async (route) => {
    if (route.request().method() === 'POST' && rejectDecision) {
      rejectDecision = false;
      await route.fulfill({
        status: 409,
        json: { reasonCode: 'device_authorization_seat_not_eligible' }
      });
      return;
    }

    await route.fallback();
  });

  await page.goto(`/devices/authorize?user_code=${userCode}`);
  await page.getByRole('radio', { name: /Personal seat/ }).check();
  await page.getByRole('button', { name: 'Approve sign-in' }).click();

  await expect(page.getByText(/seat is no longer eligible/)).toBeFocused();
  await expect(page.getByRole('heading', { level: 2, name: 'Rasmus workstation' })).toBeVisible();
  expect(state.approvalReads).toBe(2);
});

test('keeps capacity authoritative when revoking a device fails', async ({ page }) => {
  await installDeviceAuthorizationApi(page);
  await page.route('**/api/device-activations/*', async (route) => {
    await route.fulfill({
      status: 404,
      json: { reasonCode: 'device_not_active' }
    });
  });

  await page.goto(`/devices/authorize?user_code=${userCode}`);
  await page.getByRole('radio', { name: /Northstar Platform/ }).check();
  page.once('dialog', (dialog) => void dialog.accept());
  await page.getByRole('button', { name: 'Remove device Build host' }).click();

  await expect(page.getByText('That desktop device is no longer active.')).toBeFocused();
  await expect(page.getByText('3 of 3 devices active')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Make room before approving' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Approve sign-in' })).toBeDisabled();
  await expect(page.getByRole('button', { name: /^Remove device / })).toHaveCount(3);
});

test('loads a new device code after client-side navigation', async ({ page }) => {
  const state = await installDeviceAuthorizationApi(page);
  await page.goto(`/devices/authorize?user_code=${userCode}`);
  await expect(page.getByText(userCode, { exact: true })).toBeVisible();

  const nextCode = 'WXYZ-1234';
  await page.evaluate((code) => {
    window.history.pushState({}, '', `/devices/authorize?user_code=${code}`);
    window.dispatchEvent(new PopStateEvent('popstate'));
  }, nextCode);

  await expect(page.getByText(nextCode, { exact: true })).toBeVisible();
  await expect.poll(() => state.approvalReadCodes).toContain(nextCode);
});

test('ignores a delayed approval after navigation changes the device code', async ({ page }) => {
  await installDeviceAuthorizationApi(page);
  const delayed = deferredResponse();
  await page.route('**/desktop/v1/device/approval', async (route) => {
    if (route.request().method() === 'POST') {
      delayed.started();
      await delayed.release;
      await route.fulfill({ status: 204 });
      delayed.completed();
      return;
    }

    await route.fallback();
  });

  await page.goto(`/devices/authorize?user_code=${userCode}`);
  await page.getByRole('radio', { name: /Personal seat/ }).check();
  await page.getByRole('button', { name: 'Approve sign-in' }).click();
  await delayed.waitUntilStarted;
  const nextCode = 'WXYZ-APPROVE';
  await navigateToCode(page, nextCode);
  await expect(page.getByText(nextCode, { exact: true })).toBeVisible();

  delayed.resolve();
  await delayed.waitUntilCompleted;

  await expect(page.getByText(/is approved/)).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Deny sign-in' })).toBeEnabled();
});

test('ignores a delayed denial after navigation changes the device code', async ({ page }) => {
  await installDeviceAuthorizationApi(page);
  const delayed = deferredResponse();
  await page.route('**/desktop/v1/device/approval', async (route) => {
    if (route.request().method() === 'POST') {
      delayed.started();
      await delayed.release;
      await route.fulfill({ status: 204 });
      delayed.completed();
      return;
    }

    await route.fallback();
  });

  await page.goto(`/devices/authorize?user_code=${userCode}`);
  await page.getByRole('button', { name: 'Deny sign-in' }).click();
  await delayed.waitUntilStarted;
  const nextCode = 'WXYZ-DENY';
  await navigateToCode(page, nextCode);
  await expect(page.getByText(nextCode, { exact: true })).toBeVisible();

  delayed.resolve();
  await delayed.waitUntilCompleted;

  await expect(page.getByText(/authorization was denied/)).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Deny sign-in' })).toBeEnabled();
});

test('ignores a delayed device revocation after navigation changes the code', async ({ page }) => {
  await installDeviceAuthorizationApi(page);
  const delayed = deferredResponse();
  await page.route('**/api/device-activations/*', async (route) => {
    delayed.started();
    await delayed.release;
    await route.fulfill({ status: 200, json: { reasonCode: 'device_revoked' } });
    delayed.completed();
  });

  await page.goto(`/devices/authorize?user_code=${userCode}`);
  await page.getByRole('radio', { name: /Northstar Platform/ }).check();
  page.once('dialog', (dialog) => void dialog.accept());
  await page.getByRole('button', { name: 'Remove device Build host' }).click();
  await delayed.waitUntilStarted;
  const nextCode = 'WXYZ-REVOKE';
  await navigateToCode(page, nextCode);
  await expect(page.getByText(nextCode, { exact: true })).toBeVisible();

  delayed.resolve();
  await delayed.waitUntilCompleted;

  await expect(page.getByRole('button', { name: 'Remove device Build host' })).toBeEnabled();
  await expect(page.locator('.error-message')).toHaveCount(0);
});

async function installDeviceAuthorizationApi(page: Page, personalOnly = false) {
  const state: {
    approvalReads: number;
    approvalReadCodes: string[];
    approvalRequests: Request[];
    revocationRequests: Request[];
    organizationDevices: Array<{
      activationId: string;
      installationId: string;
      displayName: string;
      platform: string;
      architecture: string;
      styrhousVersion: string;
      activatedAt: string;
      lastSeenAt: string;
    }>;
  } = {
    approvalReads: 0,
    approvalReadCodes: [],
    approvalRequests: [],
    revocationRequests: [],
    organizationDevices: [
      device(1, 'Build host', '2026-08-31T10:15:00Z'),
      device(2, 'Travel laptop', '2026-09-01T08:00:00Z'),
      device(3, 'Office workstation', '2026-09-01T09:30:00Z')
    ]
  };

  await page.route('**/api/antiforgery', async (route) => {
    await route.fulfill({ json: { requestToken: 'approval-csrf-token' } });
  });

  await page.route('**/desktop/v1/device/approval**', async (route) => {
    const request = route.request();
    if (request.method() === 'GET') {
      state.approvalReads += 1;
      state.approvalReadCodes.push(new URL(request.url()).searchParams.get('user_code') ?? '');
      await route.fulfill({
        json: {
          reasonCode: 'device_authorization_awaiting_approval',
          installation: {
            installationId: '0191a8f0-1111-7000-8000-000000000080',
            displayName: 'Rasmus workstation',
            platform: 'Linux',
            architecture: 'x86_64',
            styrhousVersion: '0.1.0'
          },
          eligibleSeats: [
            {
              seatId: personalSeatId,
              billingAccountId: '0191a8f0-1111-7000-8000-000000000083',
              name: 'Personal seat',
              entitlementState: 'trial',
              entitlementReasonCode: 'active_trial',
              deviceLimit: 3,
              canActivate: true,
              activeDevices: []
            },
            {
              seatId: organizationSeatId,
              billingAccountId: '0191a8f0-1111-7000-8000-000000000084',
              name: 'Northstar Platform',
              entitlementState: 'commercial',
              entitlementReasonCode: 'active_subscription',
              deviceLimit: 3,
              canActivate: state.organizationDevices.length < 3,
              activeDevices: state.organizationDevices
            }
          ].slice(0, personalOnly ? 1 : 2),
          selectedSeatId: null
        }
      });
      return;
    }

    state.approvalRequests.push(request);
    await route.fulfill({ status: 204 });
  });

  await page.route('**/api/device-activations/*', async (route) => {
    const request = route.request();
    state.revocationRequests.push(request);
    const activationId = new URL(request.url()).pathname.split('/').at(-1);
    state.organizationDevices = state.organizationDevices.filter(
      (candidate) => candidate.activationId !== activationId
    );
    await route.fulfill({
      json: {
        reasonCode: 'device_revoked',
        activationId,
        correlationId: '0191a8f0-1111-7000-8000-000000000091'
      }
    });
  });

  return state;
}

function device(index: number, displayName: string, lastSeenAt: string) {
  const suffix = index.toString().padStart(3, '0');
  return {
    activationId: `0191a8f0-1111-7000-8000-000000000${suffix}`,
    installationId: `0191a8f0-1111-7000-8000-000000001${suffix}`,
    displayName,
    platform: 'Linux',
    architecture: 'x86_64',
    styrhousVersion: '0.1.0',
    activatedAt: '2026-08-01T10:15:00Z',
    lastSeenAt
  };
}

async function navigateToCode(page: Page, code: string) {
  await page.evaluate((nextCode) => {
    window.history.pushState({}, '', `/devices/authorize?user_code=${nextCode}`);
    window.dispatchEvent(new PopStateEvent('popstate'));
  }, code);
}

function deferredResponse() {
  let resolveRelease!: () => void;
  let signalStarted!: () => void;
  let signalCompleted!: () => void;
  const release = new Promise<void>((resolve) => {
    resolveRelease = resolve;
  });
  const waitUntilStarted = new Promise<void>((resolve) => {
    signalStarted = resolve;
  });
  const waitUntilCompleted = new Promise<void>((resolve) => {
    signalCompleted = resolve;
  });

  return {
    release,
    completed: signalCompleted,
    resolve: resolveRelease,
    started: signalStarted,
    waitUntilCompleted,
    waitUntilStarted
  };
}
