import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.route('**/api/billing-accounts', route => route.fulfill({ status: 401, json: {} }));
  await page.route('**/api/billing-prices', route => route.fulfill({ status: 401, json: {} }));
  await page.route('**/auth/providers', route => route.fulfill({ json: { providers: ['github'] } }));
});

test('renders a responsive portal shell with a stable accessibility tree', async ({ page }) => {
  await page.goto('/');

  await expect(page.getByRole('banner')).toBeVisible();
  await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toBeVisible();
  await expect(page.getByRole('heading', { level: 1, name: 'Billing' }))
    .toBeVisible();
  await expect(page.getByRole('contentinfo')).toBeVisible();
  await expect(page.getByRole('link', { name: 'Billing', exact: true })).toHaveAttribute('aria-current', 'page');

  const brandLink = page.getByRole('link', { name: 'Styrhous licensing home' });
  const brandIcon = brandLink.locator('img.brand-mark');
  const iconSource = await brandIcon.getAttribute('src');

  expect(iconSource).toMatch(/kubernetes-dev-ui.*\.svg/);
  await expect(page.locator('link[rel="icon"]')).toHaveAttribute('href', iconSource ?? '');
  expect(
    await brandIcon.evaluate(
      (image: HTMLImageElement) => image.complete && image.naturalWidth > 0 && image.naturalHeight > 0
    )
  ).toBe(true);

  expect.soft(await page.locator('body').ariaSnapshot()).toMatchSnapshot(
    'portal-shell-accessibility.txt'
  );
  await expect.soft(page).toHaveScreenshot('portal-shell.png', {
    fullPage: true
  });
});

test('supports keyboard entry and client-side section navigation', async ({ page }) => {
  await page.goto('/');
  await expect(page).toHaveURL('/billing');
  await expect(page.getByRole('link', { name: 'Styrhous licensing home' })).toBeVisible();

  await page.keyboard.press('Tab');
  await expect(page.getByRole('link', { name: 'Skip to main content' })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.locator('#main-content')).toBeFocused();
  await expect(page).toHaveURL(/#main-content$/);

  await page.goto('/');
  await expect(page.getByRole('link', { name: 'Styrhous licensing home' })).toBeVisible();
  await page.keyboard.press('Tab');
  await expect(page.getByRole('link', { name: 'Skip to main content' })).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(page.getByRole('link', { name: 'Styrhous licensing home' })).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(page.getByRole('link', { name: 'Organizations', exact: true })).toBeFocused();

  await page.evaluate(() => {
    Reflect.set(window, '__styrhousClientNavigationMarker', 'preserved');
  });

  await page.getByRole('link', { name: 'Organizations', exact: true }).click();
  await expect(page).toHaveURL('/organizations');
  await expect(page.getByRole('heading', { level: 1, name: 'Organizations' }))
    .toBeVisible();
  await expect(page.getByRole('link', { name: 'Organizations', exact: true }))
    .toHaveAttribute('aria-current', 'page');
  await expect(page.getByRole('link', { name: 'Billing', exact: true })).not.toHaveAttribute(
    'aria-current',
    'page'
  );
  expect(
    await page.evaluate(() => Reflect.get(window, '__styrhousClientNavigationMarker'))
  ).toBe('preserved');
});

const portalRoutes = [
  {
    path: '/',
    label: 'Billing',
    heading: 'Billing',
    title: 'Billing · Styrhous'
  },
  {
    path: '/organizations',
    label: 'Organizations',
    heading: 'Organizations',
    title: 'Organizations · Styrhous'
  },
  {
    path: '/devices',
    label: 'Devices',
    heading: 'Devices',
    title: 'Devices · Styrhous'
  },
  {
    path: '/billing',
    label: 'Billing',
    heading: 'Billing',
    title: 'Billing · Styrhous'
  },
  {
    path: '/account',
    label: 'Account',
    heading: 'Account',
    title: 'Account · Styrhous'
  }
] as const;

for (const route of portalRoutes) {
  test(`serves ${route.path} through the static SPA fallback`, async ({ page }) => {
    const response = await page.goto(route.path);

    expect(response?.status()).toBe(200);
    await expect(page).toHaveTitle(route.title);
    await expect(page.getByRole('heading', { level: 1, name: route.heading })).toBeVisible();
    await expect(page.getByRole('link', { name: route.label, exact: true })).toHaveAttribute(
      'aria-current',
      'page'
    );
  });
}
