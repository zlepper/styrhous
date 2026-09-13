import { expect, test, type Page, type Request } from '@playwright/test';

const organizationId = '0191a8f0-1111-7000-8000-000000000001';
const organizationSeatId = '0191a8f0-1111-7000-8000-000000000002';
const personalSeatId = '0191a8f0-1111-7000-8000-000000000003';
const ownerMembershipId = '0191a8f0-1111-7000-8000-000000000004';
const memberMembershipId = '0191a8f0-1111-7000-8000-000000000005';
const adminMembershipId = '0191a8f0-1111-7000-8000-000000000006';
const peerAdminMembershipId = '0191a8f0-1111-7000-8000-000000000007';
const createdOrganizationId = '0191a8f0-1111-7000-8000-000000000008';
const createdOwnerMembershipId = '0191a8f0-1111-7000-8000-000000000009';

function organizationSeatRow(page: Page, email: string) {
  return page
    .getByRole('table', { name: 'Organization members and invitations' })
    .getByRole('row')
    .filter({ hasText: email });
}

async function openDisclosure(page: Page, label: string) {
  const summary = page.locator('summary').filter({ hasText: label });
  if (await summary.locator('..').getAttribute('open') === null) await summary.click();
}

async function openOrganizationSeatActions(page: Page, email: string) {
  await organizationSeatRow(page, email)
    .getByRole('button', { name: `More actions for ${email}` })
    .click();
  const actions = page.getByRole('group', { name: `Actions for ${email}` });
  await expect(actions).toBeVisible();
  return actions;
}

test('manages an organization roster and invitation through the authenticated API', async (
  { page },
  testInfo
) => {
  const mutationRequests: Request[] = [];
  const state = await installOrganizationApi(page, mutationRequests);

  await page.goto('/organizations');
  await expect(page.getByRole('heading', { level: 2, name: 'Northstar Platform' })).toBeVisible();
  const seatTable = page.getByRole('table', { name: 'Organization members and invitations' });
  await expect(seatTable).toBeVisible();
  await expect(seatTable.getByRole('columnheader')).toHaveCount(7);
  await expect(seatTable.getByRole('row')).toHaveCount(4);
  await expect(organizationSeatRow(page, 'member@example.com')).toContainText('Active');
  await expect(organizationSeatRow(page, 'invitee@example.com')).toContainText(
    'Pending invitation'
  );
  await expect(page.getByRole('button', { name: /^More actions for / })).toHaveCount(2);
  await expect(
    page.getByRole('button', { name: 'More actions for member@example.com' })
  ).toHaveAttribute('title', 'More actions for member@example.com');
  const pageWidth = await page.evaluate(() => ({
    viewport: window.innerWidth,
    scroll: document.documentElement.scrollWidth
  }));
  expect(pageWidth.scroll).toBe(pageWidth.viewport);

  await expect.soft(page).toHaveScreenshot('organizations-workspace.png', { fullPage: true });
  expect.soft(await page.locator('body').ariaSnapshot()).toMatchSnapshot(
    'organizations-workspace-accessibility.txt'
  );

  if (testInfo.project.name === 'narrow-chromium') {
    const tableScroller = page.locator('.organization-seat-table-scroll');
    await page.getByRole('button', { name: 'More actions for member@example.com' }).focus();
    expect(await tableScroller.evaluate((element) => element.scrollWidth)).toBeLessThanOrEqual(await tableScroller.evaluate((element) => element.clientWidth));
    const actionBounds = await page.getByRole('button', { name: 'More actions for member@example.com' }).boundingBox();
    expect(actionBounds!.x + actionBounds!.width).toBeLessThanOrEqual(pageWidth.viewport);
    await tableScroller.evaluate((element) => element.scrollTo({ left: 0 }));
  }

  const snapshotTrigger = organizationSeatRow(page, 'invitee@example.com').getByRole('button', {
    name: 'More actions for invitee@example.com'
  });
  await snapshotTrigger.focus();
  await page.keyboard.press('Enter');
  const snapshotActions = page.getByRole('group', {
    name: 'Actions for invitee@example.com'
  });
  await expect(snapshotActions).toBeVisible();
  await expect(
    snapshotActions.getByRole('button', { name: 'Resend invitation to invitee@example.com' })
  ).toBeFocused();
  await expect(page.locator('.skip-link')).not.toBeFocused();
  await expect.soft(page).toHaveScreenshot('organizations-action-menu.png');
  expect.soft(await page.locator('body').ariaSnapshot()).toMatchSnapshot(
    'organizations-action-menu-accessibility.txt'
  );
  await page.keyboard.press('Escape');
  await expect(snapshotActions).toBeHidden();
  await expect(snapshotTrigger).toBeFocused();

  await openDisclosure(page, 'Invite people');
  await page.getByLabel('Email address').fill('new-person@example.com');
  await page.getByLabel('Role', { exact: true }).selectOption('admin');
  await expect(page.getByLabel('Include license')).toBeChecked();
  await page.getByLabel('Include license').uncheck();
  await page.getByRole('button', { name: 'Send invitation', exact: true }).click();
  await expect(page.getByText('Invitation sent to new-person@example.com.')).toBeVisible();
  await expect(organizationSeatRow(page, 'new-person@example.com')).toBeVisible();
  expect(state.invitations.at(-1)?.assignProductSeat).toBe(false);

  await page.getByLabel('License for member@example.com').selectOption('assigned');
  await expect(page.getByText('A license was assigned to member@example.com.')).toBeVisible();
  expect(
    state.members.find((member) => member.membershipId === memberMembershipId)?.productSeatAssigned
  ).toBe(true);

  await page.getByLabel('Role for member@example.com').selectOption('admin');
  await expect(page.getByText('member@example.com is now admin.')).toBeVisible();
  expect(state.members.find((member) => member.membershipId === memberMembershipId)?.role).toBe(
    'admin'
  );

  expect(mutationRequests).toHaveLength(3);
  for (const request of mutationRequests) {
    expect(request.headers()['x-csrf-token']).toBe('portal-csrf-token');
  }

  state.rejectMutations = true;
  await page.getByLabel('Role for member@example.com').selectOption('member');
  await expect(page.getByRole('heading', { name: 'Sign in to manage organizations' })).toBeVisible();
  await expect(
    page.getByText('Your Styrhous session is required to view this page.')
  ).toBeVisible();
});

test('offers an administrator only the operations allowed by backend policy', async ({ page }) => {
  const mutationRequests: Request[] = [];
  await installOrganizationApi(page, mutationRequests, 'admin');

  await page.goto('/organizations');
  await expect(page.getByRole('heading', { level: 2, name: 'Northstar Platform' })).toBeVisible();
  await expect(
    page.getByText('Organization access only. No license assigned.')
  ).toBeVisible();
  await expect(page.getByLabel(/Role for /)).toHaveCount(0);
  await expect(page.getByLabel(/License for /)).toHaveCount(4);
  await expect(page.getByLabel('License for current-admin@example.com'))
    .toHaveValue('unassigned');
  await expect(page.getByRole('button', { name: /Make .* owner/ })).toHaveCount(0);
  await expect(
    organizationSeatRow(page, 'peer-admin@example.com').getByRole('button', {
      name: 'More actions for peer-admin@example.com'
    })
  ).toHaveCount(0);

  const selfActions = await openOrganizationSeatActions(page, 'current-admin@example.com');
  await expect(
    selfActions.getByRole('button', { name: 'Leave Northstar Platform' })
  ).toBeVisible();

  const memberActions = await openOrganizationSeatActions(page, 'member@example.com');
  await expect(
    memberActions.getByRole('button', { name: 'Remove member@example.com' })
  ).toBeVisible();
  await expect(page.getByRole('button', { name: 'Remove peer-admin@example.com' })).toHaveCount(0);
  await openDisclosure(page, 'Invite people');
  await expect(page.getByRole('button', { name: 'Send invitation', exact: true })).toBeVisible();
});

test('offers an ordinary member only their own membership action', async ({ page }) => {
  const mutationRequests: Request[] = [];
  await installOrganizationApi(page, mutationRequests, 'member');

  await page.goto('/organizations');
  await expect(
    page.getByText('Organization access only. No license assigned.')
  ).toBeVisible();
  await expect(page.getByLabel(/Role for /)).toHaveCount(0);
  await expect(page.getByLabel(/License for /)).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Send invitation', exact: true })).toHaveCount(0);
  await expect(organizationSeatRow(page, 'invitee@example.com')).toHaveCount(0);
  await expect(
    organizationSeatRow(page, 'owner@example.com').getByRole('button', {
      name: 'More actions for owner@example.com'
    })
  ).toHaveCount(0);

  const selfActions = await openOrganizationSeatActions(page, 'member@example.com');
  await expect(
    selfActions.getByRole('button', { name: 'Leave Northstar Platform' })
  ).toBeVisible();
  await expect(page.getByRole('button', { name: /Make .* owner/ })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Remove owner@example.com' })).toHaveCount(0);
});

test('confirms before removing a product seat and retains organization membership', async ({
  page
}) => {
  const mutationRequests: Request[] = [];
  const state = await installOrganizationApi(page, mutationRequests);

  await page.goto('/organizations');
  const seat = page.getByLabel('License for owner@example.com');
  await expect(seat).toHaveValue('assigned');

  page.once('dialog', (dialog) => void dialog.dismiss());
  await seat.selectOption('unassigned');
  await expect(seat).toHaveValue('assigned');
  expect(mutationRequests).toHaveLength(0);

  page.once('dialog', (dialog) => void dialog.accept());
  await seat.selectOption('unassigned');
  await expect(page.getByText('The license was removed from owner@example.com.')).toBeFocused();
  await expect(page.getByText('Organization access only. No license assigned.')).toBeVisible();
  await expect(organizationSeatRow(page, 'owner@example.com')).toBeVisible();
  expect(state.members.find((member) => member.membershipId === ownerMembershipId)?.productSeatAssigned).toBe(false);
  expect(mutationRequests).toHaveLength(1);
});

test('prevents organization switching while a mutation is pending', async ({ page }) => {
  const mutationRequests: Request[] = [];
  const state = await installOrganizationApi(page, mutationRequests);
  let releaseMutation = () => {};
  state.mutationGate = new Promise<void>((resolve) => {
    releaseMutation = resolve;
  });

  await page.goto('/organizations');
  await openDisclosure(page, 'Invite people');
  await page.getByLabel('Email address').fill('pending@example.com');
  await page
    .getByRole('button', { name: 'Send invitation', exact: true })
    .click({ noWaitAfter: true });

  await expect(page.getByLabel('Current organization')).toBeDisabled();
  releaseMutation();
  await expect(page.getByText('Invitation sent to pending@example.com.')).toBeVisible();
  await expect(page.getByLabel('Current organization')).toBeEnabled();
});

test('replaces an authentication retry with one authoritative organization load', async ({
  page
}) => {
  const mutationRequests: Request[] = [];
  const state = await installOrganizationApi(page, mutationRequests);
  state.rejectReads = true;

  await page.goto('/organizations');
  await expect(page.getByRole('heading', { name: 'Sign in to manage organizations' })).toBeVisible();

  state.rejectReads = false;
  let releaseReads = () => {};
  state.readGate = new Promise<void>((resolve) => {
    releaseReads = resolve;
  });
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));

  await expect(page.getByRole('heading', { name: 'Loading organizations' })).toBeVisible();
  await expect(page.getByText('Loading memberships and seats.')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Try again' })).toHaveCount(0);
  releaseReads();
  await expect(page.getByRole('heading', { level: 2, name: 'Northstar Platform' })).toBeVisible();
});

test('explains a possible trial transfer before creating an organization', async ({ page }) => {
  await page.route('**/api/organizations', async (route) => {
    await route.fulfill({ json: { reasonCode: 'organizations_listed', organizations: [] } });
  });

  await page.goto('/organizations');

  await expect(page.getByRole('heading', { name: 'No organizations' })).toBeVisible();
  const transferDisclosure = page.getByText(
    'If this is the first organization you create and your personal trial is active, the trial moves to the organization.'
  );
  await expect(transferDisclosure).toBeVisible();
  await expect(page.getByLabel('New organization')).toHaveAttribute(
    'aria-describedby',
    'organization-trial-transfer-help'
  );
  await expect(page.locator('form.compact-form')).toHaveAttribute(
    'aria-describedby',
    'organization-trial-transfer-help'
  );
  await expect(transferDisclosure).toHaveAttribute('id', 'organization-trial-transfer-help');

  expect.soft(await page.locator('body').ariaSnapshot()).toMatchSnapshot(
    'organizations-empty-accessibility.txt'
  );
  await expect.soft(page).toHaveScreenshot('organizations-empty.png', { fullPage: true });
});

test('creates an organization, reports a trial transfer, and switches workspaces', async ({ page }) => {
  const mutationRequests: Request[] = [];
  await installOrganizationApi(page, mutationRequests);

  await page.goto('/organizations');
  await openDisclosure(page, 'New organization');
  await page.getByLabel('New organization').fill('Release Engineering');
  await page.getByRole('button', { name: 'Create organization' }).click();

  const transferStatus = page.getByText(
    'Release Engineering is ready. Your active personal trial was transferred to it.'
  );
  await expect(transferStatus).toBeVisible();
  await expect(transferStatus).toBeFocused();
  await expect(page.getByRole('heading', { level: 2, name: 'Release Engineering' })).toBeVisible();
  await expect(organizationSeatRow(page, 'owner@example.com')).toBeVisible();

  await page.getByLabel('Current organization').selectOption(organizationId);
  await expect(organizationSeatRow(page, 'member@example.com')).toBeVisible();
  await page.getByLabel('Current organization').selectOption(createdOrganizationId);
  await expect(organizationSeatRow(page, 'member@example.com')).toHaveCount(0);

  expect(mutationRequests).toHaveLength(1);
  expect(mutationRequests[0].headers()['x-csrf-token']).toBe('portal-csrf-token');
});

test('confirms and applies invitation and member removal actions', async ({ page }) => {
  const mutationRequests: Request[] = [];
  await installOrganizationApi(page, mutationRequests);

  await page.goto('/organizations');
  page.once('dialog', (dialog) => void dialog.dismiss());
  let actions = await openOrganizationSeatActions(page, 'member@example.com');
  await actions.getByRole('button', { name: 'Remove member@example.com' }).click();
  await expect(organizationSeatRow(page, 'member@example.com')).toBeVisible();
  expect(mutationRequests).toHaveLength(0);

  actions = await openOrganizationSeatActions(page, 'invitee@example.com');
  await actions.getByRole('button', { name: 'Resend invitation to invitee@example.com' }).click();
  await expect(
    page.getByText('A fresh invitation was sent to invitee@example.com.')
  ).toBeVisible();
  await expect(organizationSeatRow(page, 'invitee@example.com')).toContainText(
    'Sep 4, 2026, 12:00 PM'
  );

  page.once('dialog', (dialog) => void dialog.accept());
  actions = await openOrganizationSeatActions(page, 'invitee@example.com');
  await actions.getByRole('button', { name: 'Cancel invitation to invitee@example.com' }).click();
  await expect(
    page.getByText('The invitation for invitee@example.com was cancelled.')
  ).toBeVisible();
  await expect(organizationSeatRow(page, 'invitee@example.com')).toHaveCount(0);

  page.once('dialog', (dialog) => void dialog.accept());
  actions = await openOrganizationSeatActions(page, 'member@example.com');
  await actions.getByRole('button', { name: 'Remove member@example.com' }).click();
  await expect(page.getByText('member@example.com was removed.')).toBeFocused();
  await expect(organizationSeatRow(page, 'member@example.com')).toHaveCount(0);

  expect(mutationRequests).toHaveLength(3);
  for (const request of mutationRequests) {
    expect(request.headers()['x-csrf-token']).toBe('portal-csrf-token');
  }
});

test("transfers ownership and refreshes the current user's capabilities", async ({ page }) => {
  const mutationRequests: Request[] = [];
  await installOrganizationApi(page, mutationRequests);

  await page.goto('/organizations');
  page.once('dialog', (dialog) => void dialog.accept());
  const actions = await openOrganizationSeatActions(page, 'member@example.com');
  await actions.getByRole('button', { name: 'Make member@example.com owner' }).click();

  await expect(page.getByText('member@example.com is now the organization owner.')).toBeFocused();
  await expect(
    page.getByText('3 members and invitations · Admin')
  ).toBeVisible();
  await expect(page.getByLabel(/Role for /)).toHaveCount(0);
  const currentUserActions = await openOrganizationSeatActions(page, 'owner@example.com');
  await expect(
    currentUserActions.getByRole('button', { name: 'Leave Northstar Platform' })
  ).toBeVisible();
  expect(mutationRequests).toHaveLength(1);
});

test('associates a general invitation API error with its form', async ({ page }) => {
  const mutationRequests: Request[] = [];
  const state = await installOrganizationApi(page, mutationRequests);
  state.rejectReasonCode = 'seat_capacity_reached';

  await page.goto('/organizations');
  await openDisclosure(page, 'Invite people');
  const email = page.getByLabel('Email address');
  await email.fill('blocked@example.com');
  await page.getByRole('button', { name: 'Send invitation', exact: true }).click();

  await expect(page.getByRole('alert')).toHaveText(
    'All available organization seats are assigned or reserved.'
  );
  await expect(email).not.toHaveAttribute('aria-invalid', 'true');
  await expect(
    page.getByRole('button', { name: 'Send invitation', exact: true }).locator('..')
  ).toHaveAttribute('aria-describedby', 'organization-action-error');
});

test('marks and clears an invitation email validation error', async ({ page }) => {
  const mutationRequests: Request[] = [];
  const state = await installOrganizationApi(page, mutationRequests);
  state.validationErrors = { email: ['A valid email address is required.'] };

  await page.goto('/organizations');
  await openDisclosure(page, 'Invite people');
  const email = page.getByLabel('Email address');
  await email.fill('invalid@example.com');
  await page.getByRole('button', { name: 'Send invitation', exact: true }).click();

  await expect(page.getByRole('alert')).toHaveText('A valid email address is required.');
  await expect(email).toHaveAttribute('aria-invalid', 'true');
  await expect(email).toHaveAttribute('aria-describedby', 'organization-action-error');

  await email.fill('corrected@example.com');
  await expect(email).not.toHaveAttribute('aria-invalid', 'true');
  await expect(page.getByRole('alert')).toHaveCount(0);
});

test('clears field error context when switching organizations', async ({ page }) => {
  const mutationRequests: Request[] = [];
  const state = await installOrganizationApi(page, mutationRequests);

  await page.goto('/organizations');
  await openDisclosure(page, 'New organization');
  await page.getByLabel('New organization').fill('Release Engineering');
  await page.getByRole('button', { name: 'Create organization' }).click();

  state.validationErrors = { email: ['A valid email address is required.'] };
  await openDisclosure(page, 'Invite people');
  await page.getByLabel('Email address').fill('invalid@example.com');
  await page.getByRole('button', { name: 'Send invitation', exact: true }).click();
  await expect(page.getByLabel('Email address')).toHaveAttribute('aria-invalid', 'true');

  await page.getByLabel('Current organization').selectOption(organizationId);
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(page.getByLabel('Email address')).not.toHaveAttribute('aria-invalid', 'true');
  await expect(page.getByLabel('Email address')).not.toHaveAttribute(
    'aria-describedby',
    'organization-action-error'
  );
});

test('restores a role selector when the backend rejects the change', async ({ page }) => {
  const mutationRequests: Request[] = [];
  const state = await installOrganizationApi(page, mutationRequests);
  state.rejectReasonCode = 'organization_members_changed';

  await page.goto('/organizations');
  const role = page.getByLabel('Role for member@example.com');
  await role.selectOption('admin');

  await expect(page.getByRole('alert')).toHaveText(
    'The roster changed while you were editing it. Review it and try again.'
  );
  await expect(role).toHaveValue('member');
});

test('restores a product seat selector when the backend rejects the change', async ({ page }) => {
  const mutationRequests: Request[] = [];
  const state = await installOrganizationApi(page, mutationRequests);
  state.rejectReasonCode = 'seat_capacity_reached';

  await page.goto('/organizations');
  const seat = page.getByLabel('License for member@example.com');
  await expect(seat).toHaveValue('unassigned');
  await seat.selectOption('assigned');

  await expect(page.getByRole('alert')).toHaveText(
    'All available organization seats are assigned or reserved.'
  );
  await expect(seat).toHaveValue('unassigned');
  expect(
    state.members.find((member) => member.membershipId === memberMembershipId)
      ?.productSeatAssigned
  ).toBe(false);
});

test('shows every seat and revokes an active desktop device', async ({ page }) => {
  const mutationRequests: Request[] = [];
  await installDeviceApi(page, mutationRequests);

  await page.goto('/devices');
  await expect(page.getByRole('heading', { level: 2, name: 'Personal seat' })).toBeVisible();
  await expect(page.getByRole('heading', { level: 2, name: 'Northstar Platform' })).toBeVisible();
  await expect(page.getByRole('heading', { level: 3, name: 'Workstation 14' })).toBeVisible();
  const laptop = page.getByRole('listitem').filter({ has: page.getByRole('heading', { name: 'Laptop', exact: true }) });
  await expect(laptop.getByText('arm64')).not.toBeVisible();
  await laptop.locator('summary').click();
  await expect(laptop.getByText('arm64')).toBeVisible();
  await laptop.locator('summary').click();
  await expect(page.getByText('Assign a license to restore licensed access on these devices.')).toBeVisible();
  await expect(page.getByText('License inactive', { exact: true }).first()).toBeVisible();

  expect.soft(await page.locator('body').ariaSnapshot()).toMatchSnapshot(
    'devices-workspace-accessibility.txt'
  );
  await expect.soft(page).toHaveScreenshot('devices-workspace.png', { fullPage: true });

  page.once('dialog', (dialog) => void dialog.accept());
  await page.getByRole('button', { name: 'Remove device Workstation 14' }).click();
  await expect(page.getByText('Workstation 14 was removed.')).toBeFocused();
  await expect(page.getByRole('heading', { level: 3, name: 'Workstation 14' })).toHaveCount(0);

  expect(mutationRequests).toHaveLength(1);
  expect(mutationRequests[0].headers()['x-csrf-token']).toBe('portal-csrf-token');
});

test('keeps a device visible when revocation is rejected', async ({ page }) => {
  const mutationRequests: Request[] = [];
  const state = await installDeviceApi(page, mutationRequests);
  state.rejectRevocations = true;

  await page.goto('/devices');
  page.once('dialog', (dialog) => void dialog.accept());
  await page.getByRole('button', { name: 'Remove device Workstation 14' }).click();

  await expect(page.getByRole('alert')).toHaveText(
    'That desktop device is no longer active.'
  );
  await expect(page.getByRole('heading', { level: 3, name: 'Workstation 14' })).toBeVisible();
});

test('replaces an authentication retry with one authoritative device load', async ({ page }) => {
  const mutationRequests: Request[] = [];
  const state = await installDeviceApi(page, mutationRequests);
  state.rejectReads = true;

  await page.goto('/devices');
  await expect(page.getByRole('heading', { name: 'Sign in to review active devices' })).toBeVisible();

  state.rejectReads = false;
  let releaseReads = () => {};
  state.readGate = new Promise<void>((resolve) => {
    releaseReads = resolve;
  });
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));

  await expect(page.getByRole('heading', { name: 'Loading devices' })).toBeVisible();
  await expect(page.getByText('Loading seats and active devices.')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Try again' })).toHaveCount(0);
  releaseReads();
  await expect(page.getByRole('heading', { level: 2, name: 'Personal seat' })).toBeVisible();
});

test('shows personal and organization billing state from the authenticated API', async ({
  page
}) => {
  await installBillingApi(page);

  await page.goto('/billing');
  await expect(page.getByRole('heading', { level: 2, name: 'Personal plan' })).toBeVisible();
  await expect(page.getByRole('heading', { level: 2, name: 'Northstar Platform' })).toBeVisible();
  await expect(
    page.getByRole('article', { name: 'Personal plan' })
      .getByText('Payment overdue', { exact: true })
  ).toBeVisible();
  const overdue = page.getByRole('article', { name: 'Northstar Platform' }).getByText('Payment overdue', { exact: true });
  await expect(overdue).toHaveClass(/warning/);
  await expect(overdue).not.toHaveClass(/eligible/);
  await expect(page.getByText('Provider status', { exact: true })).toHaveCount(0);
  await expect(page.getByLabel('Billing period for Release Cooperative')).toHaveValue('monthly');
  await expect(page.getByLabel('Seats for Release Cooperative')).toHaveValue('4');
  await expect(page.getByLabel('Seats for Release Cooperative')).not.toHaveAttribute('max');
  await expect(
    page.getByRole('button', { name: 'Continue to checkout' })
  ).toBeVisible();
  await expect(
    page.getByRole('article', { name: 'Personal plan' })
      .getByRole('button', { name: 'Manage billing' })
  ).toBeVisible();
  await expect(
    page.getByRole('article', { name: 'Northstar Platform' })
      .getByRole('button', { name: 'Manage billing' })
  ).toHaveCount(0);
  await expect(page.getByLabel('Purchased seats for Growth Studio')).toHaveValue('6');
  await expect(page.getByLabel('Purchased seats for Growth Studio')).toHaveAttribute(
    'max',
    '2147483647'
  );
  const seatChangeForm = page.getByRole('form', { name: 'Change seats for Growth Studio' });
  const seatInputBox = await seatChangeForm.getByLabel(
    'Purchased seats for Growth Studio'
  ).boundingBox();
  const seatButtonBox = await seatChangeForm.getByRole(
    'button',
    { name: 'Update purchased seats' }
  ).boundingBox();
  expect(seatInputBox).not.toBeNull();
  expect(seatButtonBox).not.toBeNull();
  expect(seatInputBox!.width).toBeGreaterThan(seatButtonBox!.width * 0.9);
  await expect(
    page.getByRole('article', { name: 'Growth Studio' })
      .getByRole('button', { name: 'Update purchased seats' })
  ).toBeVisible();
  await expect(
    page.getByText('Only the owner can change billing.')
  ).toBeVisible();

  expect.soft(await page.locator('body').ariaSnapshot()).toMatchSnapshot(
    'billing-workspace-accessibility.txt'
  );
  await expect.soft(page).toHaveScreenshot('billing-workspace.png', { fullPage: true });
});

test('previews subscription prices when period and seat count change', async ({ page }) => {
  const state = await installBillingApi(page);
  await page.goto('/billing');
  const purchase = page.getByRole('form', { name: 'Purchase Release Cooperative' });
  await expect(purchase.getByText('USD 48.00 / month', { exact: false })).toBeVisible();
  await purchase.getByLabel('Billing period for Release Cooperative').selectOption('annual');
  await expect(purchase.getByText('USD 480.00 / year', { exact: false })).toBeVisible();
  await purchase.getByLabel('Seats for Release Cooperative').fill('5');
  await expect(purchase.getByText('USD 600.00 / year', { exact: false })).toBeVisible();
  expect(state.checkoutRequests).toHaveLength(0);
});

test('keeps checkout available when price discovery is unavailable', async ({ page }) => {
  await installBillingApi(page);
  await page.route('**/api/billing-prices', route => route.fulfill({ status: 503, json: {} }));
  await page.goto('/billing');
  const purchase = page.getByRole('form', { name: 'Purchase Release Cooperative' });
  await expect(purchase.getByText('Price available at checkout.')).toBeVisible();
  await expect(purchase.getByRole('button', { name: 'Continue to checkout' })).toBeEnabled();
});

test('opens Stripe billing management with antiforgery and no browser-owned inputs', async ({
  page
}) => {
  const state = await installBillingApi(page);

  await page.goto('/billing');
  await page.getByRole('article', { name: 'Personal plan' })
    .getByRole('button', { name: 'Manage billing' })
    .click();
  await expect(page).toHaveURL('https://billing.stripe.test/session');

  expect(state.customerPortalRequests).toHaveLength(1);
  expect(state.customerPortalRequests[0].headers()['x-csrf-token']).toBe(
    'portal-csrf-token'
  );
  expect(state.customerPortalRequests[0].postData()).toBeNull();
});

test('changes organization seats and reloads the authoritative account state', async ({ page }) => {
  const state = await installBillingApi(page);

  await page.goto('/billing');
  const account = page.getByRole('article', { name: 'Growth Studio' });
  await account.getByLabel('Purchased seats for Growth Studio').fill('9');
  await account.getByRole('button', { name: 'Update purchased seats' }).click();

  await expect(account.getByRole('status')).toHaveText('Purchased seats changed to 9.');
  await expect(account.getByLabel('Purchased seats for Growth Studio')).toHaveValue('9');
  expect(state.seatQuantityRequests).toHaveLength(1);
  expect(state.seatQuantityRequests[0].headers()['x-csrf-token']).toBe('portal-csrf-token');
  expect(state.seatQuantityRequests[0].postDataJSON()).toEqual({
    seatQuantity: 9,
    billingOperationId: null
  });
});

test('keeps a newer authoritative quantity after a successful seat response', async ({ page }) => {
  const state = await installBillingApi(page);
  state.projectAfterSeatChangeTo = 7;

  await page.goto('/billing');
  const account = page.getByRole('article', { name: 'Growth Studio' });
  const form = account.getByRole('form', { name: 'Change seats for Growth Studio' });
  const initialReads = state.billingAccountReads;
  await account.getByLabel('Purchased seats for Growth Studio').fill('9');
  await account.getByRole('button', { name: 'Update purchased seats' }).click();

  await expect.poll(() => state.currentSeatQuantity).toBe(7);
  await expect.poll(() => state.billingAccountReads).toBeGreaterThan(initialReads);
  await expect(account.getByLabel('Purchased seats for Growth Studio')).toHaveValue('7');
  await expect(form.getByRole('status')).toHaveCount(0);
  await expect(account.getByText('5 / 7', { exact: true })).toBeVisible();
});

test('retries a seat provider failure with the same durable operation', async ({ page }) => {
  const state = await installBillingApi(page);
  state.failNextSeatQuantity = true;

  await page.goto('/billing');
  const account = page.getByRole('article', { name: 'Growth Studio' });
  await account.getByLabel('Purchased seats for Growth Studio').fill('8');
  await account.getByRole('button', { name: 'Update purchased seats' }).click();
  await expect(account.getByRole('alert')).toHaveText(
    'The seat change could not be completed. Retry the same change.'
  );
  await expect(account.getByRole('alert')).toBeFocused();
  await account.getByRole('button', { name: 'Retry seat change' }).click();

  await expect(account.getByRole('status')).toHaveText('Purchased seats changed to 8.');
  expect(state.seatQuantityRequests).toHaveLength(2);
  expect(state.seatQuantityRequests[0].postDataJSON()).toMatchObject({
    billingOperationId: null
  });
  expect(state.seatQuantityRequests[1].postDataJSON()).toMatchObject({
    billingOperationId: '0191a8f0-1111-7000-8000-000000000062'
  });
});

test('retains the original seat operation when replay requires reconciliation', async ({ page }) => {
  const state = await installBillingApi(page);
  state.failNextSeatQuantity = true;
  state.requireSeatQuantityReconciliation = true;

  await page.goto('/billing');
  const account = page.getByRole('article', { name: 'Growth Studio' });
  await account.getByLabel('Purchased seats for Growth Studio').fill('8');
  await account.getByRole('button', { name: 'Update purchased seats' }).click();
  await expect(account.getByRole('alert')).toHaveText(
    'The seat change could not be completed. Retry the same change.'
  );

  await account.getByRole('button', { name: 'Retry seat change' }).click();
  await expect(account.getByRole('alert')).toHaveText(
    'This seat change may already have completed. Check again later or contact support; another update will not be submitted automatically.'
  );
  await expect(account.getByRole('button', { name: 'Retry seat change' })).toBeVisible();

  await account.getByRole('button', { name: 'Retry seat change' }).click();
  await expect(account.getByRole('alert')).toHaveText(
    'This seat change may already have completed. Check again later or contact support; another update will not be submitted automatically.'
  );
  expect(state.seatQuantityRequests).toHaveLength(3);
  expect(state.seatQuantityRequests[1].postDataJSON()).toMatchObject({
    billingOperationId: '0191a8f0-1111-7000-8000-000000000062'
  });
  expect(state.seatQuantityRequests[2].postDataJSON()).toMatchObject({
    billingOperationId: '0191a8f0-1111-7000-8000-000000000062'
  });
});

test('recovers a changed seat draft onto the one live operation', async ({ page }) => {
  const state = await installBillingApi(page);
  state.failNextSeatQuantity = true;
  state.returnLiveSeatQuantityOperation = true;

  await page.goto('/billing');
  const account = page.getByRole('article', { name: 'Growth Studio' });
  const input = account.getByLabel('Purchased seats for Growth Studio');
  await input.fill('8');
  await account.getByRole('button', { name: 'Update purchased seats' }).click();
  await input.fill('10');
  await account.getByRole('button', { name: 'Update purchased seats' }).click();

  await expect(account.getByRole('alert')).toHaveText(
    'A seat change is already pending. Continue that exact change before starting another.'
  );
  await expect(input).toHaveValue('8');
  await account.getByRole('button', { name: 'Retry seat change' }).click();
  await expect(account.getByRole('status')).toHaveText('Purchased seats changed to 8.');
  expect(state.seatQuantityRequests).toHaveLength(3);
});

test('prevents duplicate seat updates while Stripe is responding', async ({ page }) => {
  const state = await installBillingApi(page);
  let releaseSeatQuantity = () => {};
  state.seatQuantityGate = new Promise<void>((resolve) => {
    releaseSeatQuantity = resolve;
  });

  await page.goto('/billing');
  const account = page.getByRole('article', { name: 'Growth Studio' });
  await account.getByLabel('Purchased seats for Growth Studio').fill('7');
  await account.getByRole('button', { name: 'Update purchased seats' }).click();
  await expect(account.getByRole('button', { name: 'Updating seats…' }))
    .toHaveAttribute('aria-disabled', 'true');
  await account.getByRole('form', { name: 'Change seats for Growth Studio' })
    .evaluate((form: HTMLFormElement) => form.requestSubmit());
  expect(state.seatQuantityRequests).toHaveLength(1);

  releaseSeatQuantity();
  await expect(account.getByRole('status')).toHaveText('Purchased seats changed to 7.');
});

test('raises a seat decrease to the server-required reservation capacity', async ({ page }) => {
  const state = await installBillingApi(page);
  state.requiredSeatQuantity = 7;

  await page.goto('/billing');
  const account = page.getByRole('article', { name: 'Growth Studio' });
  const input = account.getByLabel('Purchased seats for Growth Studio');
  await input.fill('5');
  await account.getByRole('button', { name: 'Update purchased seats' }).click();

  await expect(account.getByRole('alert')).toHaveText(
    'Choose at least 7 seats to cover product seats in use and seat-bearing invitations.'
  );
  await expect(account.getByRole('alert')).toBeFocused();
  await expect(input).toHaveValue('7');
});

test('reloads an externally superseded seat quantity before another attempt', async ({ page }) => {
  const state = await installBillingApi(page);
  state.supersedeNextSeatQuantityWith = 7;

  await page.goto('/billing');
  const account = page.getByRole('article', { name: 'Growth Studio' });
  await account.getByLabel('Purchased seats for Growth Studio').fill('9');
  await account.getByRole('button', { name: 'Update purchased seats' }).click();

  await expect(account.getByRole('alert')).toHaveText(
    'The seat count changed. Review the updated subscription before trying again.'
  );
  await expect(account.getByRole('alert')).toBeFocused();
  await expect(account.getByLabel('Purchased seats for Growth Studio')).toHaveValue('7');
});

test('keeps a newer authoritative quantity after a superseded seat response', async ({ page }) => {
  const state = await installBillingApi(page);
  state.supersedeNextSeatQuantityWith = 7;
  state.projectAfterSeatChangeTo = 8;

  await page.goto('/billing');
  const account = page.getByRole('article', { name: 'Growth Studio' });
  await account.getByLabel('Purchased seats for Growth Studio').fill('9');
  await account.getByRole('button', { name: 'Update purchased seats' }).click();

  await expect(account.getByRole('alert')).toHaveText(
    'The seat count changed. Review the updated subscription before trying again.'
  );
  await expect(account.getByLabel('Purchased seats for Growth Studio')).toHaveValue('8');
  await expect(account.getByText('5 / 8', { exact: true })).toBeVisible();
});

test('shows sign-in state when the seat-management session expires', async ({ page }) => {
  const state = await installBillingApi(page);
  state.rejectNextSeatQuantityAuthentication = true;

  await page.goto('/billing');
  const account = page.getByRole('article', { name: 'Growth Studio' });
  await account.getByLabel('Purchased seats for Growth Studio').fill('7');
  await account.getByRole('button', { name: 'Update purchased seats' }).click();

  await expect(page.getByRole('heading', { name: 'Sign in to review billing' })).toBeVisible();
  await expect(page.getByRole('article')).toHaveCount(0);
});

test('does not finish a seat update after leaving its billing route', async ({ page }) => {
  const state = await installBillingApi(page);
  let releaseSeatQuantity = () => {};
  state.seatQuantityGate = new Promise<void>((resolve) => {
    releaseSeatQuantity = resolve;
  });

  await page.goto('/billing');
  const account = page.getByRole('article', { name: 'Growth Studio' });
  await account.getByLabel('Purchased seats for Growth Studio').fill('7');
  await account.getByRole('button', { name: 'Update purchased seats' }).click();
  await expect.poll(() => state.seatQuantityRequests.length).toBe(1);
  const abortedRequest = page.waitForEvent('requestfailed', {
    predicate: (request) => request.url().includes('/seat-quantity')
  });

  await page.getByRole('link', { name: 'Account', exact: true }).click();
  await expect(page).toHaveURL('/account');
  releaseSeatQuantity();
  await abortedRequest;
  await expect(page.getByRole('heading', { name: 'Account', exact: true }))
    .toBeVisible();
});

test('recovers a Stripe billing-management provider outage locally', async ({ page }) => {
  const state = await installBillingApi(page);
  state.failNextCustomerPortal = true;

  await page.goto('/billing');
  const personalAccount = page.getByRole('article', { name: 'Personal plan' });
  await personalAccount.getByRole('button', { name: 'Manage billing' }).click();
  await expect(personalAccount.getByRole('alert')).toHaveText(
    'Billing management is temporarily unavailable. Try again.'
  );
  await expect(personalAccount.getByRole('alert')).toBeFocused();

  await personalAccount.getByRole('button', { name: 'Manage billing' }).click();
  await expect(page).toHaveURL('https://billing.stripe.test/session');
  expect(state.customerPortalRequests).toHaveLength(2);
});

test('shows sign-in state when the billing-management session expires', async ({ page }) => {
  const state = await installBillingApi(page);
  state.rejectNextCustomerPortalAuthentication = true;

  await page.goto('/billing');
  await page.getByRole('article', { name: 'Personal plan' })
    .getByRole('button', { name: 'Manage billing' })
    .click();

  await expect(page.getByRole('heading', { name: 'Sign in to review billing' })).toBeVisible();
  await expect(page.getByRole('article')).toHaveCount(0);
});

test('prevents duplicate submission while Stripe billing management is opening', async ({
  page
}) => {
  const state = await installBillingApi(page);
  let releaseCustomerPortal = () => {};
  state.customerPortalGate = new Promise<void>((resolve) => {
    releaseCustomerPortal = resolve;
  });

  await page.goto('/billing');
  const personalAccount = page.getByRole('article', { name: 'Personal plan' });
  await personalAccount.getByRole('button', { name: 'Manage billing' }).click();
  const pendingButton = personalAccount.getByRole('button', {
    name: 'Opening billing…'
  });
  await expect(pendingButton).toHaveAttribute('aria-disabled', 'true');
  await expect.poll(() => state.customerPortalRequests.length).toBe(1);
  await personalAccount.getByRole('form', { name: 'Manage billing for Personal plan' })
    .evaluate((form: HTMLFormElement) => form.requestSubmit());
  expect(state.customerPortalRequests).toHaveLength(1);

  releaseCustomerPortal();
  await expect(page).toHaveURL('https://billing.stripe.test/session');
  expect(state.customerPortalRequests).toHaveLength(1);
});

test('does not redirect after leaving an in-flight billing-management request', async ({
  page
}) => {
  const state = await installBillingApi(page);
  let releaseCustomerPortal = () => {};
  state.customerPortalGate = new Promise<void>((resolve) => {
    releaseCustomerPortal = resolve;
  });

  await page.goto('/billing');
  await page.getByRole('article', { name: 'Personal plan' })
    .getByRole('button', { name: 'Manage billing' })
    .click();
  await expect.poll(() => state.customerPortalRequests.length).toBe(1);
  const abortedRequest = page.waitForEvent('requestfailed', {
    predicate: (request) => request.url().includes('/customer-portal-sessions')
  });

  await page.getByRole('link', { name: 'Account', exact: true }).click();
  await expect(page).toHaveURL('/account');
  await expect(page.getByRole('heading', { name: 'Account', exact: true }))
    .toBeVisible();
  releaseCustomerPortal();
  await abortedRequest;
  await expect(page).toHaveURL('/account');
  await expect(page.getByRole('heading', { name: 'Account', exact: true }))
    .toBeVisible();
});

test('retries billing after an authentication challenge', async ({ page }) => {
  const state = await installBillingApi(page);
  state.rejectReads = true;
  await page.route('**/api/billing-prices', route => state.rejectReads
    ? route.fulfill({ status: 401, json: {} }) : route.fallback());

  await page.goto('/billing');
  await expect(page.getByRole('heading', { name: 'Sign in to review billing' })).toBeVisible();

  state.rejectReads = false;
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.getByRole('heading', { level: 2, name: 'Personal plan' })).toBeVisible();
  await expect(page.getByText('USD 48.00 / month', { exact: false })).toBeVisible();
});

test('shows one authoritative loading state while billing is fetched', async ({ page }) => {
  const state = await installBillingApi(page);
  let releaseRead = () => {};
  state.readGate = new Promise<void>((resolve) => {
    releaseRead = resolve;
  });

  await page.goto('/billing');
  await expect(page.getByRole('heading', { name: 'Loading billing accounts' })).toBeVisible();
  await expect(page.getByText('Loading trials, subscriptions, and seat assignments.')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Try again' })).toHaveCount(0);

  releaseRead();
  await expect(page.getByRole('heading', { level: 2, name: 'Personal plan' })).toBeVisible();
});

test('recovers billing after an ordinary service error', async ({ page }) => {
  const state = await installBillingApi(page);
  state.failReads = true;

  await page.goto('/billing');
  await expect(page.getByRole('heading', { name: 'Billing could not be loaded' })).toBeVisible();
  await expect(page.getByRole('alert')).toHaveText(
    'The licensing service returned HTTP 500.'
  );

  state.failReads = false;
  await page.getByRole('button', { name: 'Try again' }).click();
  await expect(page.getByRole('heading', { level: 2, name: 'Personal plan' })).toBeVisible();
  await expect(page.getByRole('alert')).toHaveCount(0);
});

test('does not describe an ineligible provider window as access', async ({ page }) => {
  const state = await installBillingApi(page);
  state.showCapacityExceededOnly = true;

  await page.goto('/billing');
  await expect(page.getByText('Seat above purchased capacity', { exact: true })).toBeVisible();
  await expect(page.getByText('Billing period ends', { exact: true })).toBeVisible();
  await expect(page.getByText('Access until', { exact: true })).toHaveCount(0);
});

test('labels a seatless organization entitlement directly', async ({ page }) => {
  const state = await installBillingApi(page);
  state.showSeatlessOnly = true;

  await page.goto('/billing');
  await expect(
    page.getByRole('article', { name: 'Seatless Operations' })
      .getByText('No license assigned', { exact: true })
  ).toBeVisible();
});

test('starts annual Checkout with the selected organization quantity', async ({ page }) => {
  const state = await installBillingApi(page);

  await page.goto('/billing');
  await page.getByLabel('Billing period for Release Cooperative').selectOption('annual');
  await page.getByLabel('Seats for Release Cooperative').fill('12');
  await page.getByRole('button', { name: 'Continue to checkout' }).click();
  await expect(page).toHaveURL('https://checkout.stripe.test/session');

  expect(state.checkoutRequests).toHaveLength(1);
  expect(state.checkoutRequests[0].headers()['x-csrf-token']).toBe('portal-csrf-token');
  expect(state.checkoutRequests[0].postDataJSON()).toEqual({
    cadence: 'annual',
    seatQuantity: 12,
    billingOperationId: null
  });
});

test('retries a provider failure with the same durable Checkout operation', async ({ page }) => {
  const state = await installBillingApi(page);
  state.failNextCheckout = true;

  await page.goto('/billing');
  await page.getByRole('button', { name: 'Continue to checkout' }).click();
  await expect(page.getByRole('alert')).toHaveText(
    'Checkout is temporarily unavailable. Try again.'
  );
  await page.getByRole('button', { name: 'Try checkout again' }).click();
  await expect(page).toHaveURL('https://checkout.stripe.test/session');

  expect(state.checkoutRequests).toHaveLength(2);
  expect(state.checkoutRequests[0].postDataJSON()).toMatchObject({
    billingOperationId: null
  });
  expect(state.checkoutRequests[1].postDataJSON()).toMatchObject({
    billingOperationId: '0191a8f0-1111-7000-8000-000000000047'
  });
});

test('prevents duplicate submission while checkout is opening', async ({ page }) => {
  const state = await installBillingApi(page);
  let releaseCheckout = () => {};
  state.checkoutGate = new Promise<void>((resolve) => {
    releaseCheckout = resolve;
  });

  await page.goto('/billing');
  await page.getByRole('button', { name: 'Continue to checkout' }).click();
  const pendingButton = page.getByRole('button', { name: 'Opening checkout…' });
  await expect(pendingButton).toHaveAttribute('aria-disabled', 'true');
  await page.getByRole('form', { name: 'Purchase Release Cooperative' }).evaluate(
    (form: HTMLFormElement) => form.requestSubmit()
  );
  expect(state.checkoutRequests).toHaveLength(1);
  releaseCheckout();
  await expect(page).toHaveURL('https://checkout.stripe.test/session');
});

test('recovers a changed draft onto the one live Checkout operation', async ({ page }) => {
  const state = await installBillingApi(page);
  state.failNextCheckout = true;
  state.returnLiveOperationForChangedDraft = true;

  await page.goto('/billing');
  await page.getByRole('button', { name: 'Continue to checkout' }).click();
  await page.getByLabel('Billing period for Release Cooperative').selectOption('annual');
  await page.getByLabel('Seats for Release Cooperative').fill('12');
  await page.getByRole('button', { name: 'Continue to checkout' }).click();

  await expect(page.getByRole('alert')).toHaveText(
    'A recent checkout is still active. Continue that attempt before changing its cadence or seats.'
  );
  await expect(page.getByLabel('Billing period for Release Cooperative')).toHaveValue('monthly');
  await expect(page.getByLabel('Seats for Release Cooperative')).toHaveValue('4');
  await page.getByRole('button', { name: 'Try checkout again' }).click();
  await expect(page).toHaveURL('https://checkout.stripe.test/session');

  expect(state.checkoutRequests).toHaveLength(3);
  expect(state.checkoutRequests[1].postDataJSON()).toMatchObject({
    cadence: 'annual',
    seatQuantity: 12,
    billingOperationId: null
  });
  expect(state.checkoutRequests[2].postDataJSON()).toMatchObject({
    cadence: 'monthly',
    seatQuantity: 4,
    billingOperationId: '0191a8f0-1111-7000-8000-000000000047'
  });
});

test('blocks replacement until an undersized live Checkout expires', async ({ page }) => {
  const state = await installBillingApi(page);
  state.failNextCheckout = true;
  state.returnCapacityChangedForRetry = true;
  state.capacityChangedExpiresAt = new Date(Date.now() + 1_500).toISOString();

  await page.goto('/billing');
  await page.getByRole('button', { name: 'Continue to checkout' }).click();
  await page.getByRole('button', { name: 'Try checkout again' }).click();

  await expect(page.getByRole('alert')).toContainText(
    'This checkout has too few seats for your members and invitations.'
  );
  await expect(page.getByLabel('Seats for Release Cooperative')).toHaveValue('6');
  await expect(page.getByRole('button', { name: 'Wait for current Checkout to expire' }))
    .toHaveAttribute('aria-disabled', 'true');
  expect(state.checkoutRequests).toHaveLength(2);

  const replacementButton = page.getByRole('button', {
    name: 'Continue to checkout'
  });
  await expect(replacementButton).toHaveAttribute('aria-disabled', 'false');
  await replacementButton.click();
  await expect(page).toHaveURL('https://checkout.stripe.test/session');
  expect(state.checkoutRequests).toHaveLength(3);
});

test('starts fixed one-seat Checkout for a personal billing account', async ({ page }) => {
  const state = await installBillingApi(page);
  state.showPurchasablePersonalOnly = true;

  await page.goto('/billing');
  await expect(page.getByText('One personal seat')).toBeVisible();
  await page.getByRole('button', { name: 'Continue to checkout' }).click();
  await expect(page).toHaveURL('https://checkout.stripe.test/session');

  expect(state.checkoutRequests).toHaveLength(1);
  expect(state.checkoutRequests[0].postDataJSON()).toEqual({
    cadence: 'monthly',
    seatQuantity: 1,
    billingOperationId: null
  });
});

for (const status of ['canceled', 'incomplete_expired']) {
  test(`offers a new purchase after a ${status} subscription`, async ({ page }) => {
    const state = await installBillingApi(page);
    state.showPurchasablePersonalOnly = true;
    state.terminalPersonalStatus = status;
    await page.goto('/billing');
    await expect(page.getByRole('button', { name: 'Continue to checkout' })).toBeVisible();
    if (status === 'canceled') {
      expect.soft(await page.locator('body').ariaSnapshot()).toMatchSnapshot('billing-repurchase-accessibility.txt');
      await expect.soft(page).toHaveScreenshot('billing-repurchase.png', { fullPage: true });
    }
    await page.getByRole('button', { name: 'Continue to checkout' }).click();
    await expect(page).toHaveURL('https://checkout.stripe.test/session');
    expect(state.checkoutRequests[0].postDataJSON().seatQuantity).toBe(1);
  });
}

test('repurchases an organization subscription while retaining invoice access', async ({ page }) => {
  const state = await installBillingApi(page);
  state.showTerminalOrganizationOnly = true;
  await page.goto('/billing');
  await expect(page.getByRole('button', { name: 'Manage billing' })).toBeVisible();
  await expect(page.getByRole('form', { name: 'Change seats for Growth Studio' })).toHaveCount(0);
  await page.getByLabel('Seats for Growth Studio').fill('8');
  await page.getByRole('button', { name: 'Continue to checkout' }).click();
  await expect(page).toHaveURL('https://checkout.stripe.test/session');
  expect(state.checkoutRequests[0].postDataJSON().seatQuantity).toBe(8);
});

test('raises an undersized organization purchase to the server-required quantity', async ({
  page
}) => {
  const state = await installBillingApi(page);
  state.requiredSeatQuantity = 6;

  await page.goto('/billing');
  await page.getByRole('button', { name: 'Continue to checkout' }).click();

  await expect(page.getByRole('alert')).toHaveText(
    'Choose at least 6 seats to cover product seats in use and seat-bearing invitations.'
  );
  await expect(page.getByLabel('Seats for Release Cooperative')).toHaveValue('6');
  await expect(page.getByRole('alert')).toBeFocused();
  await expect(page).toHaveURL(/\/billing$/);
});

test('shows explicit Checkout return state without claiming immediate projection', async ({
  page
}) => {
  await installBillingApi(page);

  await page.goto('/billing?checkout=success');
  await expect(page.getByRole('status')).toHaveText(
    'Checkout completed. Your billing status will update after payment is confirmed.'
  );

  await page.goto('/billing?checkout=cancelled');
  await expect(page.getByRole('status')).toHaveText(
    'Checkout was cancelled. No subscription changes were made.'
  );
});

async function installOrganizationApi(
  page: Page,
  mutationRequests: Request[],
  actorRole: 'owner' | 'admin' | 'member' = 'owner'
) {
  const currentMembershipId =
    actorRole === 'owner'
      ? ownerMembershipId
      : actorRole === 'admin'
        ? adminMembershipId
        : memberMembershipId;
  const organizations = [
    {
      organizationId,
      billingAccountId: '0191a8f0-1111-7000-8000-000000000010',
      membershipId: currentMembershipId,
      seatId:
        actorRole === 'owner'
          ? organizationSeatId
          : actorRole === 'admin'
            ? '0191a8f0-1111-7000-8000-000000000017'
            : '0191a8f0-1111-7000-8000-000000000013',
      name: 'Northstar Platform',
      role: actorRole,
      productSeatAssigned: actorRole === 'owner',
      deviceLimit: 3,
      joinedAt: '2026-09-01T10:15:00Z'
    }
  ];
  const members = [
    {
      membershipId: ownerMembershipId,
      userId: '0191a8f0-1111-7000-8000-000000000011',
      seatId: organizationSeatId,
      email: 'owner@example.com',
      role: 'owner',
      productSeatAssigned: true,
      deviceLimit: 3,
      joinedAt: '2026-09-01T10:15:00Z'
    },
    ...(actorRole === 'admin'
      ? [
          {
            membershipId: adminMembershipId,
            userId: '0191a8f0-1111-7000-8000-000000000018',
            seatId: '0191a8f0-1111-7000-8000-000000000017',
            email: 'current-admin@example.com',
            role: 'admin',
            productSeatAssigned: false,
            deviceLimit: 3,
            joinedAt: '2026-09-02T08:00:00Z'
          },
          {
            membershipId: peerAdminMembershipId,
            userId: '0191a8f0-1111-7000-8000-000000000019',
            seatId: '0191a8f0-1111-7000-8000-000000000020',
            email: 'peer-admin@example.com',
            role: 'admin',
            productSeatAssigned: false,
            deviceLimit: 3,
            joinedAt: '2026-09-02T08:15:00Z'
          }
        ]
      : []),
    {
      membershipId: memberMembershipId,
      userId: '0191a8f0-1111-7000-8000-000000000012',
      seatId: '0191a8f0-1111-7000-8000-000000000013',
      email: 'member@example.com',
      role: 'member',
      productSeatAssigned: false,
      deviceLimit: 3,
      joinedAt: '2026-09-02T08:30:00Z'
    }
  ];
  const invitations = [
    {
      invitationId: '0191a8f0-1111-7000-8000-000000000014',
      createdByUserId: '0191a8f0-1111-7000-8000-000000000011',
      email: 'invitee@example.com',
      role: 'member',
      assignProductSeat: false,
      createdAt: '2026-09-03T09:00:00Z',
      lastSentAt: '2026-09-03T09:00:00Z',
      expiresAt: '2026-09-10T09:00:00Z'
    }
  ];
  const createdMembers = [
    {
      membershipId: createdOwnerMembershipId,
      userId: '0191a8f0-1111-7000-8000-000000000011',
      seatId: '0191a8f0-1111-7000-8000-000000000036',
      email: 'owner@example.com',
      role: 'owner',
      productSeatAssigned: true,
      deviceLimit: 3,
      joinedAt: '2026-09-04T13:00:00Z'
    }
  ];

  const state: {
    members: typeof members;
    invitations: typeof invitations;
    rejectMutations: boolean;
    rejectReasonCode: string | null;
    validationErrors: Record<string, string[]> | null;
    rejectReads: boolean;
    readGate: Promise<void> | null;
    mutationGate: Promise<void> | null;
  } = {
    members,
    invitations,
    rejectMutations: false,
    rejectReasonCode: null,
    validationErrors: null,
    rejectReads: false,
    readGate: null,
    mutationGate: null
  };

  await page.route('**/api/**', async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname;
    if (path === '/api/antiforgery') {
      await route.fulfill({ json: { requestToken: 'portal-csrf-token' } });
      return;
    }
    if (state.rejectReads && request.method() === 'GET') {
      await route.fulfill({ status: 401 });
      return;
    }
    if (state.readGate && request.method() === 'GET') await state.readGate;
    if (state.rejectMutations && request.method() !== 'GET') {
      await route.fulfill({ status: 401 });
      return;
    }
    if (state.rejectReasonCode && request.method() !== 'GET') {
      await route.fulfill({
        status: 409,
        json: { reasonCode: state.rejectReasonCode }
      });
      return;
    }
    if (state.validationErrors && request.method() !== 'GET') {
      await route.fulfill({
        status: 400,
        json: { title: 'Validation failed', errors: state.validationErrors }
      });
      return;
    }
    if (path === '/api/organizations' && request.method() === 'GET') {
      await route.fulfill({ json: { reasonCode: 'organizations_listed', organizations } });
      return;
    }
    if (path === `/api/organizations/${organizationId}/members` && request.method() === 'GET') {
      await route.fulfill({
        json: { reasonCode: 'members_listed', organizationId, members }
      });
      return;
    }
    if (
      path === `/api/organizations/${createdOrganizationId}/members` &&
      request.method() === 'GET'
    ) {
      await route.fulfill({
        json: {
          reasonCode: 'organization_members_listed',
          organizationId: createdOrganizationId,
          members: createdMembers
        }
      });
      return;
    }
    if (
      path === `/api/organizations/${createdOrganizationId}/invitations` &&
      request.method() === 'GET'
    ) {
      await route.fulfill({
        json: {
          reasonCode: 'organization_invitations_listed',
          organizationId: createdOrganizationId,
          invitations: []
        }
      });
      return;
    }
    const invitationMatch = path.match(
      new RegExp(`^/api/organizations/${organizationId}/invitations/([^/]+)(/resend)?$`)
    );
    if (invitationMatch && request.method() === 'POST' && invitationMatch[2] === '/resend') {
      mutationRequests.push(request);
      const invitation = invitations.find(
        (candidate) => candidate.invitationId === invitationMatch[1]
      );
      if (invitation) invitation.lastSentAt = '2026-09-04T12:00:00Z';
      await route.fulfill({
        json: {
          reasonCode: 'invitation_resent',
          invitationId: invitationMatch[1],
          correlationId: '0191a8f0-1111-7000-8000-000000000032',
          lastSentAt: '2026-09-04T12:00:00Z'
        }
      });
      return;
    }
    if (invitationMatch && request.method() === 'DELETE') {
      mutationRequests.push(request);
      const invitationIndex = invitations.findIndex(
        (candidate) => candidate.invitationId === invitationMatch[1]
      );
      if (invitationIndex >= 0) invitations.splice(invitationIndex, 1);
      await route.fulfill({
        json: {
          reasonCode: 'invitation_cancelled',
          invitationId: invitationMatch[1],
          correlationId: '0191a8f0-1111-7000-8000-000000000033'
        }
      });
      return;
    }
    if (
      path === `/api/organizations/${organizationId}/invitations` &&
      request.method() === 'GET'
    ) {
      await route.fulfill({
        json: { reasonCode: 'invitations_listed', organizationId, invitations }
      });
      return;
    }
    if (
      path ===
        `/api/organizations/${organizationId}/members/${memberMembershipId}/transfer-ownership` &&
      request.method() === 'POST'
    ) {
      mutationRequests.push(request);
      const owner = members.find((candidate) => candidate.membershipId === ownerMembershipId);
      const member = members.find((candidate) => candidate.membershipId === memberMembershipId);
      if (owner) owner.role = 'admin';
      if (member) member.role = 'owner';
      organizations[0].role = 'admin';
      await route.fulfill({
        json: {
          reasonCode: 'organization_ownership_transferred',
          organizationId,
          previousOwnerMembershipId: ownerMembershipId,
          ownerMembershipId: memberMembershipId,
          correlationId: '0191a8f0-1111-7000-8000-000000000034'
        }
      });
      return;
    }
    if (
      path === `/api/organizations/${organizationId}/members/${memberMembershipId}` &&
      request.method() === 'DELETE'
    ) {
      mutationRequests.push(request);
      const memberIndex = members.findIndex(
        (candidate) => candidate.membershipId === memberMembershipId
      );
      if (memberIndex >= 0) members.splice(memberIndex, 1);
      await route.fulfill({
        json: {
          reasonCode: 'organization_member_removed',
          organizationId,
          membershipId: memberMembershipId,
          correlationId: '0191a8f0-1111-7000-8000-000000000035'
        }
      });
      return;
    }
    if (
      path === `/api/organizations/${organizationId}/invitations` &&
      request.method() === 'POST'
    ) {
      mutationRequests.push(request);
      if (state.mutationGate) await state.mutationGate;
      const body = request.postDataJSON() as {
        email: string;
        role: 'admin' | 'member';
        assignProductSeat: boolean;
      };
      invitations.push({
        invitationId: '0191a8f0-1111-7000-8000-000000000015',
        createdByUserId: '0191a8f0-1111-7000-8000-000000000011',
        email: body.email,
        role: body.role,
        assignProductSeat: body.assignProductSeat,
        createdAt: '2026-09-04T11:00:00Z',
        lastSentAt: '2026-09-04T11:00:00Z',
        expiresAt: '2026-09-11T11:00:00Z'
      });
      await route.fulfill({
        status: 201,
        json: {
          reasonCode: 'invitation_created',
          invitationId: '0191a8f0-1111-7000-8000-000000000015',
          correlationId: '0191a8f0-1111-7000-8000-000000000016',
          expiresAt: '2026-09-11T11:00:00Z'
        }
      });
      return;
    }
    if (path === '/api/organizations' && request.method() === 'POST') {
      mutationRequests.push(request);
      const body = request.postDataJSON() as { name: string };
      organizations.push({
        organizationId: createdOrganizationId,
        billingAccountId: '0191a8f0-1111-7000-8000-000000000037',
        membershipId: createdOwnerMembershipId,
        seatId: '0191a8f0-1111-7000-8000-000000000036',
        name: body.name,
        role: 'owner',
        productSeatAssigned: true,
        deviceLimit: 3,
        joinedAt: '2026-09-04T13:00:00Z'
      });
      await route.fulfill({
        status: 201,
        json: {
          userId: '0191a8f0-1111-7000-8000-000000000011',
          organizationId: createdOrganizationId,
          billingAccountId: '0191a8f0-1111-7000-8000-000000000037',
          ownerMembershipId: createdOwnerMembershipId,
          seatId: '0191a8f0-1111-7000-8000-000000000036',
          correlationId: '0191a8f0-1111-7000-8000-000000000038',
          trialWasTransferred: true
        }
      });
      return;
    }
    if (
      path.match(
        new RegExp(`^/api/organizations/${organizationId}/members/([^/]+)/seat$`)
      ) &&
      request.method() === 'PATCH'
    ) {
      mutationRequests.push(request);
      const membershipId = path.split('/').at(-2) ?? '';
      const body = request.postDataJSON() as { assigned: boolean };
      const member = members.find((candidate) => candidate.membershipId === membershipId);
      if (member) member.productSeatAssigned = body.assigned;
      if (organizations[0].membershipId === membershipId) {
        organizations[0].productSeatAssigned = body.assigned;
      }
      await route.fulfill({
        json: {
          reasonCode: body.assigned
            ? 'organization_member_seat_assigned'
            : 'organization_member_seat_unassigned',
          organizationId,
          membershipId,
          userId: member?.userId,
          seatId: member?.seatId,
          assigned: body.assigned,
          deviceLimit: member?.deviceLimit,
          correlationId: '0191a8f0-1111-7000-8000-000000000039',
          changedAt: '2026-09-04T14:00:00Z'
        }
      });
      return;
    }
    if (
      path ===
        `/api/organizations/${organizationId}/members/${memberMembershipId}/role` &&
      request.method() === 'PATCH'
    ) {
      mutationRequests.push(request);
      const body = request.postDataJSON() as { role: 'admin' | 'member' };
      const member = members.find((candidate) => candidate.membershipId === memberMembershipId);
      if (member) member.role = body.role;
      await route.fulfill({
        json: {
          reasonCode: 'member_role_changed',
          organizationId,
          membershipId: memberMembershipId,
          role: body.role
        }
      });
      return;
    }

    await route.fulfill({ status: 404, json: { reasonCode: 'not_mocked' } });
  });

  return state;
}

async function installDeviceApi(page: Page, mutationRequests: Request[]) {
  const organization = {
    organizationId,
    billingAccountId: '0191a8f0-1111-7000-8000-000000000010',
    membershipId: ownerMembershipId,
    seatId: organizationSeatId,
    name: 'Northstar Platform',
    role: 'owner',
    productSeatAssigned: false,
    deviceLimit: 3,
    joinedAt: '2026-09-01T10:15:00Z'
  };
  const devicesBySeat = new Map([
    [
      personalSeatId,
      [
        {
          activationId: '0191a8f0-1111-7000-8000-000000000021',
          installationId: '0191a8f0-1111-7000-8000-000000000022',
          displayName: 'Laptop',
          platform: 'macOS',
          architecture: 'arm64',
          styrhousVersion: '0.8.0',
          activatedAt: '2026-08-20T14:00:00Z',
          lastSeenAt: '2026-09-04T07:45:00Z'
        }
      ]
    ],
    [
      organizationSeatId,
      [
        {
          activationId: '0191a8f0-1111-7000-8000-000000000023',
          installationId: '0191a8f0-1111-7000-8000-000000000024',
          displayName: 'Workstation 14',
          platform: 'Linux',
          architecture: 'x86_64',
          styrhousVersion: '0.8.0',
          activatedAt: '2026-08-28T09:20:00Z',
          lastSeenAt: '2026-09-04T08:05:00Z'
        }
      ]
    ]
  ]);
  const state: {
    rejectRevocations: boolean;
    rejectReads: boolean;
    readGate: Promise<void> | null;
  } = { rejectRevocations: false, rejectReads: false, readGate: null };

  await page.route('**/api/**', async (route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    if (path === '/api/antiforgery') {
      await route.fulfill({ json: { requestToken: 'portal-csrf-token' } });
      return;
    }
    if (state.rejectReads && request.method() === 'GET') {
      await route.fulfill({ status: 401 });
      return;
    }
    if (state.readGate && request.method() === 'GET') await state.readGate;
    if (path === '/api/organizations') {
      await route.fulfill({
        json: { reasonCode: 'organizations_listed', organizations: [organization] }
      });
      return;
    }
    if (path === '/api/entitlements') {
      await route.fulfill({
        json: {
          entitlements: [
            {
              seatId: personalSeatId,
              billingAccountId: '0191a8f0-1111-7000-8000-000000000030',
              state: 'trial',
              reasonCode: 'active_trial',
              isEligible: true,
              validFrom: '2026-08-15T10:15:00Z',
              validUntil: '2026-09-14T10:15:00Z'
            },
            {
              seatId: organizationSeatId,
              billingAccountId: organization.billingAccountId,
              state: 'evaluation',
              reasonCode: 'product_seat_not_assigned',
              isEligible: false,
              validFrom: null,
              validUntil: null
            }
          ]
        }
      });
      return;
    }
    const seatMatch = path.match(/^\/api\/seats\/([^/]+)\/devices$/);
    if (seatMatch) {
      const seatId = seatMatch[1];
      await route.fulfill({
        json: {
          reasonCode: 'devices_listed',
          seatId,
          deviceLimit: 3,
          activeDevices: devicesBySeat.get(seatId) ?? []
        }
      });
      return;
    }
    const activationMatch = path.match(/^\/api\/device-activations\/([^/]+)$/);
    if (activationMatch && request.method() === 'DELETE') {
      if (state.rejectRevocations) {
        await route.fulfill({ status: 404, json: { reasonCode: 'device_not_active' } });
        return;
      }
      mutationRequests.push(request);
      for (const [seatId, devices] of devicesBySeat) {
        devicesBySeat.set(
          seatId,
          devices.filter((device) => device.activationId !== activationMatch[1])
        );
      }
      await route.fulfill({
        json: {
          reasonCode: 'device_revoked',
          activationId: activationMatch[1],
          correlationId: '0191a8f0-1111-7000-8000-000000000031'
        }
      });
      return;
    }

    await route.fulfill({ status: 404, json: { reasonCode: 'not_mocked' } });
  });

  return state;
}

async function installBillingApi(page: Page) {
  await page.route('**/api/billing-prices', route => route.fulfill({ json: { prices: [
    { cadence: 'monthly', unitAmount: 12, currency: 'USD', taxIncluded: false },
    { cadence: 'annual', unitAmount: 120, currency: 'USD', taxIncluded: false }
  ] } }));
  const state: {
    rejectReads: boolean;
    failReads: boolean;
    readGate: Promise<void> | null;
    showCapacityExceededOnly: boolean;
    showSeatlessOnly: boolean;
    showPurchasablePersonalOnly: boolean;
    terminalPersonalStatus: string | null;
    showTerminalOrganizationOnly: boolean;
    checkoutRequests: Request[];
    checkoutGate: Promise<void> | null;
    failNextCheckout: boolean;
    customerPortalRequests: Request[];
    customerPortalGate: Promise<void> | null;
    failNextCustomerPortal: boolean;
    rejectNextCustomerPortalAuthentication: boolean;
    returnLiveOperationForChangedDraft: boolean;
    returnCapacityChangedForRetry: boolean;
    capacityChangedExpiresAt: string;
    requiredSeatQuantity: number | null;
    seatQuantityRequests: Request[];
    seatQuantityGate: Promise<void> | null;
    failNextSeatQuantity: boolean;
    requireSeatQuantityReconciliation: boolean;
    returnLiveSeatQuantityOperation: boolean;
    currentSeatQuantity: number;
    pendingSeatQuantity: number | null;
    rejectNextSeatQuantityAuthentication: boolean;
    supersedeNextSeatQuantityWith: number | null;
    projectAfterSeatChangeTo: number | null;
    billingAccountReads: number;
  } = {
    rejectReads: false,
    failReads: false,
    readGate: null,
    showCapacityExceededOnly: false,
    showSeatlessOnly: false,
    showPurchasablePersonalOnly: false,
    showTerminalOrganizationOnly: false,
    terminalPersonalStatus: null as string | null,
    checkoutRequests: [],
    checkoutGate: null,
    failNextCheckout: false,
    customerPortalRequests: [],
    customerPortalGate: null,
    failNextCustomerPortal: false,
    rejectNextCustomerPortalAuthentication: false,
    returnLiveOperationForChangedDraft: false,
    returnCapacityChangedForRetry: false,
    capacityChangedExpiresAt: '2026-09-01T10:50:00Z',
    requiredSeatQuantity: null,
    seatQuantityRequests: [],
    seatQuantityGate: null,
    failNextSeatQuantity: false,
    requireSeatQuantityReconciliation: false,
    returnLiveSeatQuantityOperation: false,
    currentSeatQuantity: 6,
    pendingSeatQuantity: null,
    rejectNextSeatQuantityAuthentication: false,
    supersedeNextSeatQuantityWith: null,
    projectAfterSeatChangeTo: null,
    billingAccountReads: 0
  };
  const accounts = [
    {
      billingAccountId: '0191a8f0-1111-7000-8000-000000000040',
      accountKind: 'personal',
      organizationId: null,
      organizationName: null,
      organizationRole: null,
      canManageBilling: true,
      canStartCheckout: false,
      assignedSeatCount: 1,
      entitlement: {
        seatId: personalSeatId,
        billingAccountId: '0191a8f0-1111-7000-8000-000000000040',
        state: 'trial',
        reasonCode: 'active_trial',
        isEligible: true,
        validFrom: '2026-08-15T10:15:00Z',
        validUntil: '2026-09-14T10:15:00Z'
      },
      trial: {
        trialId: '0191a8f0-1111-7000-8000-000000000041',
        startedAt: '2026-08-15T10:15:00Z',
        endsAt: '2026-09-14T10:15:00Z',
        terminatedAt: null,
        transferredAt: null,
        isActive: true
      },
      subscription: {
        subscriptionId: '0191a8f0-1111-7000-8000-000000000044',
        status: 'unpaid',
        seatQuantity: 1,
        cancelAtPeriodEnd: false,
        currentPeriodStartedAt: '2026-09-01T00:00:00Z',
        currentPeriodEndsAt: '2026-10-01T00:00:00Z',
        projectedAt: '2026-09-04T08:00:00Z'
      }
    },
    {
      billingAccountId: '0191a8f0-1111-7000-8000-000000000042',
      accountKind: 'organization',
      organizationId,
      organizationName: 'Northstar Platform',
      organizationRole: 'admin',
      canManageBilling: false,
      canStartCheckout: false,
      assignedSeatCount: 3,
      entitlement: {
        seatId: organizationSeatId,
        billingAccountId: '0191a8f0-1111-7000-8000-000000000042',
        state: 'grace',
        reasonCode: 'subscription_past_due',
        isEligible: true,
        validFrom: '2026-09-01T00:00:00Z',
        validUntil: '2026-10-01T00:00:00Z'
      },
      trial: null,
      subscription: {
        subscriptionId: '0191a8f0-1111-7000-8000-000000000043',
        status: 'past_due',
        seatQuantity: 4,
        cancelAtPeriodEnd: false,
        currentPeriodStartedAt: '2026-09-01T00:00:00Z',
        currentPeriodEndsAt: '2026-10-01T00:00:00Z',
        projectedAt: '2026-09-04T08:00:00Z'
      }
    },
    {
      billingAccountId: '0191a8f0-1111-7000-8000-000000000045',
      accountKind: 'organization',
      organizationId: '0191a8f0-1111-7000-8000-000000000046',
      organizationName: 'Release Cooperative',
      organizationRole: 'owner',
      canManageBilling: true,
      canStartCheckout: true,
      assignedSeatCount: 4,
      entitlement: {
        seatId: '0191a8f0-1111-7000-8000-000000000048',
        billingAccountId: '0191a8f0-1111-7000-8000-000000000045',
        state: 'trial',
        reasonCode: 'active_trial',
        isEligible: true,
        validFrom: '2026-08-20T10:15:00Z',
        validUntil: '2026-09-19T10:15:00Z'
      },
      trial: {
        trialId: '0191a8f0-1111-7000-8000-000000000049',
        startedAt: '2026-08-20T10:15:00Z',
        endsAt: '2026-09-19T10:15:00Z',
        terminatedAt: null,
        transferredAt: '2026-08-22T10:15:00Z',
        isActive: true
      },
      subscription: null
    },
    {
      billingAccountId: '0191a8f0-1111-7000-8000-000000000063',
      accountKind: 'organization',
      organizationId: '0191a8f0-1111-7000-8000-000000000064',
      organizationName: 'Growth Studio',
      organizationRole: 'owner',
      canManageBilling: true,
      canStartCheckout: false,
      assignedSeatCount: 5,
      entitlement: {
        seatId: '0191a8f0-1111-7000-8000-000000000065',
        billingAccountId: '0191a8f0-1111-7000-8000-000000000063',
        state: 'commercial',
        reasonCode: 'active_subscription',
        isEligible: true,
        validFrom: '2026-09-01T00:00:00Z',
        validUntil: '2026-10-01T00:00:00Z'
      },
      trial: null,
      subscription: {
        subscriptionId: '0191a8f0-1111-7000-8000-000000000066',
        status: 'active',
        seatQuantity: 6,
        cancelAtPeriodEnd: false,
        currentPeriodStartedAt: '2026-09-01T00:00:00Z',
        currentPeriodEndsAt: '2026-10-01T00:00:00Z',
        projectedAt: '2026-09-04T08:00:00Z'
      }
    }
  ];
  const capacityExceededAccount = {
    ...accounts[1],
    organizationName: 'Capacity Limited',
    entitlement: {
      ...accounts[1].entitlement,
      state: 'evaluation',
      reasonCode: 'subscription_seat_capacity_exceeded',
      isEligible: false
    }
  };
  const seatlessAccount = {
    ...accounts[1],
    organizationName: 'Seatless Operations',
    assignedSeatCount: 0,
    entitlement: {
      ...accounts[1].entitlement,
      state: 'evaluation',
      reasonCode: 'product_seat_not_assigned',
      isEligible: false,
      validFrom: null,
      validUntil: null
    }
  };

  await page.route('**/api/billing-accounts', async (route) => {
    state.billingAccountReads++;
    await state.readGate;
    if (state.rejectReads) {
      await route.fulfill({ status: 401 });
      return;
    }
    if (state.failReads) {
      await route.fulfill({ status: 500 });
      return;
    }

    const listedAccounts = accounts.map((account) =>
      account.organizationName === 'Growth Studio'
        ? {
            ...account,
            subscription: account.subscription
              ? { ...account.subscription, seatQuantity: state.currentSeatQuantity }
              : null
          }
        : account
    );
    await route.fulfill({
      json: {
        reasonCode: 'billing_accounts_listed',
        accounts: state.showTerminalOrganizationOnly
          ? [{ ...accounts[3], canStartCheckout: true, subscription: { ...accounts[3].subscription, status: 'canceled' } }]
          : state.showCapacityExceededOnly
          ? [capacityExceededAccount]
          : state.showSeatlessOnly
            ? [seatlessAccount]
          : state.showPurchasablePersonalOnly
            ? [{ ...accounts[0], canStartCheckout: true, subscription: state.terminalPersonalStatus
              ? { ...accounts[0].subscription, status: state.terminalPersonalStatus } : null }]
            : listedAccounts
      }
    });
  });

  await page.route('**/api/antiforgery', async (route) => {
    await route.fulfill({ json: { requestToken: 'portal-csrf-token' } });
  });

  await page.route('**/api/billing-accounts/*/checkout-sessions', async (route) => {
    const request = route.request();
    state.checkoutRequests.push(request);
    const body = request.postDataJSON() as {
      cadence: 'monthly' | 'annual';
      seatQuantity: number;
      billingOperationId: string | null;
    };
    if (state.checkoutGate) await state.checkoutGate;
    if (state.requiredSeatQuantity && body.seatQuantity < state.requiredSeatQuantity) {
      await route.fulfill({
        status: 409,
        json: {
          reasonCode: 'seat_quantity_too_small',
          requiredSeatQuantity: state.requiredSeatQuantity
        }
      });
      return;
    }

    if (state.failNextCheckout) {
      state.failNextCheckout = false;
      await route.fulfill({
        status: 503,
        json: {
          reasonCode: 'checkout_provider_unavailable',
          billingOperationId: '0191a8f0-1111-7000-8000-000000000047'
        }
      });
      return;
    }

    if (state.returnLiveOperationForChangedDraft && body.billingOperationId === null) {
      state.returnLiveOperationForChangedDraft = false;
      await route.fulfill({
        status: 409,
        json: {
          reasonCode: 'checkout_operation_in_progress',
          billingOperationId: '0191a8f0-1111-7000-8000-000000000047',
          cadence: 'monthly',
          seatQuantity: 4,
          expiresAt: '2026-09-01T10:50:00Z'
        }
      });
      return;
    }


    if (state.returnCapacityChangedForRetry && body.billingOperationId !== null) {
      state.returnCapacityChangedForRetry = false;
      await route.fulfill({
        status: 409,
        json: {
          reasonCode: 'checkout_operation_capacity_changed',
          billingOperationId: '0191a8f0-1111-7000-8000-000000000047',
          cadence: 'monthly',
          seatQuantity: 4,
          requiredSeatQuantity: 6,
          expiresAt: state.capacityChangedExpiresAt
        }
      });
      return;
    }

    await route.fulfill({
      json: {
        reasonCode: 'checkout_session_created',
        billingOperationId:
          body.billingOperationId ?? '0191a8f0-1111-7000-8000-000000000047',
        redirectUrl: 'https://checkout.stripe.test/session'
      }
    });
  });

  await page.route('**/api/billing-accounts/*/customer-portal-sessions', async (route) => {
    const request = route.request();
    state.customerPortalRequests.push(request);
    if (state.customerPortalGate) await state.customerPortalGate;
    if (state.rejectNextCustomerPortalAuthentication) {
      state.rejectNextCustomerPortalAuthentication = false;
      await route.fulfill({ status: 401 });
      return;
    }
    if (state.failNextCustomerPortal) {
      state.failNextCustomerPortal = false;
      await route.fulfill({
        status: 503,
        json: { reasonCode: 'customer_portal_provider_unavailable' }
      });
      return;
    }

    await route.fulfill({
      json: {
        reasonCode: 'customer_portal_session_created',
        redirectUrl: 'https://billing.stripe.test/session'
      }
    });
  });

  await page.route('**/api/billing-accounts/*/seat-quantity', async (route) => {
    const request = route.request();
    state.seatQuantityRequests.push(request);
    const body = request.postDataJSON() as {
      seatQuantity: number;
      billingOperationId: string | null;
    };
    if (state.seatQuantityGate) await state.seatQuantityGate;
    if (state.rejectNextSeatQuantityAuthentication) {
      state.rejectNextSeatQuantityAuthentication = false;
      await route.fulfill({ status: 401 });
      return;
    }
    if (state.requiredSeatQuantity && body.seatQuantity < state.requiredSeatQuantity) {
      await route.fulfill({
        status: 409,
        json: {
          reasonCode: 'seat_quantity_too_small',
          requiredSeatQuantity: state.requiredSeatQuantity
        }
      });
      return;
    }
    if (state.supersedeNextSeatQuantityWith) {
      const supersededQuantity = state.supersedeNextSeatQuantityWith;
      state.currentSeatQuantity = state.projectAfterSeatChangeTo ?? supersededQuantity;
      state.supersedeNextSeatQuantityWith = null;
      state.projectAfterSeatChangeTo = null;
      await route.fulfill({
        status: 409,
        json: {
          reasonCode: 'subscription_quantity_changed',
          seatQuantity: supersededQuantity
        }
      });
      return;
    }
    if (state.failNextSeatQuantity) {
      state.failNextSeatQuantity = false;
      state.pendingSeatQuantity = body.seatQuantity;
      await route.fulfill({
        status: 503,
        json: {
          reasonCode: 'seat_quantity_provider_unavailable',
          billingOperationId: '0191a8f0-1111-7000-8000-000000000062',
          previousSeatQuantity: state.currentSeatQuantity,
          seatQuantity: body.seatQuantity
        }
      });
      return;
    }

    if (state.requireSeatQuantityReconciliation) {
      await route.fulfill({
        status: 409,
        json: {
          reasonCode: 'seat_quantity_reconciliation_required',
          billingOperationId: '0191a8f0-1111-7000-8000-000000000062',
          previousSeatQuantity: state.currentSeatQuantity,
          seatQuantity: body.seatQuantity
        }
      });
      return;
    }

    if (state.returnLiveSeatQuantityOperation && body.billingOperationId === null) {
      state.returnLiveSeatQuantityOperation = false;
      await route.fulfill({
        status: 409,
        json: {
          reasonCode: 'seat_quantity_operation_in_progress',
          billingOperationId: '0191a8f0-1111-7000-8000-000000000062',
          previousSeatQuantity: state.currentSeatQuantity,
          seatQuantity: state.pendingSeatQuantity
        }
      });
      return;
    }

    state.currentSeatQuantity = state.projectAfterSeatChangeTo ?? body.seatQuantity;
    state.projectAfterSeatChangeTo = null;
    state.pendingSeatQuantity = null;
    await route.fulfill({
      json: {
        reasonCode: 'seat_quantity_changed',
        billingOperationId:
          body.billingOperationId ?? '0191a8f0-1111-7000-8000-000000000062',
        seatQuantity: body.seatQuantity
      }
    });
  });

  await page.route('https://checkout.stripe.test/session', async (route) => {
    await route.fulfill({
      contentType: 'text/html',
      body: '<!doctype html><title>Checkout</title><h1>Checkout</h1>'
    });
  });

  await page.route('https://billing.stripe.test/session', async (route) => {
    await route.fulfill({
      contentType: 'text/html',
      body: '<!doctype html><title>Billing</title><h1>Stripe billing management</h1>'
    });
  });

  return state;
}
