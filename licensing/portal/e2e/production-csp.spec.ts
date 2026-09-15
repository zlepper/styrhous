import { readFile } from 'node:fs/promises';
import { expect, test } from '@playwright/test';

// The ASP.NET Core monolith owns response headers. This test verifies the portal's
// generated meta policy, which is the policy that protects its static document.
for (const [path, heading] of [
  ['/account', 'Account'],
  ['/billing', 'Billing'],
  ['/devices/authorize', 'Authorize device']
]) {
  test(`production CSP permits startup at ${path} and blocks injected scripts`, async ({ page }) => {
    const document = await readFile('build/index.html', 'utf8');
    let injectScript = false;
    const violations: string[] = [];
    page.on('console', (message) => {
      if (message.text().includes('Content Security Policy')) violations.push(message.text());
    });
    await page.route('**/*', async (route) => {
      if (!route.request().isNavigationRequest()) return route.fallback();
      const body = injectScript
        ? document.replace('</body>', '<script>document.documentElement.dataset.injected = "yes"</script></body>')
        : document;
      await route.fulfill({ body, contentType: 'text/html' });
    });
    await page.route('**/auth/session', (route) => route.fulfill({
      json: { authenticated: false, configuredProviders: ['github'] }
    }));
    await page.route('**/auth/providers', (route) => route.fulfill({ json: { providers: ['github'] } }));
    await page.route('**/api/**', (route) => route.fulfill({ status: 401 }));
    await page.goto(path);
    await expect(page.getByRole('heading', { name: heading, exact: true })).toBeVisible();
    expect(violations).toEqual([]);
    injectScript = true;
    await page.reload();
    await expect(page.getByRole('heading', { name: heading, exact: true })).toBeVisible();
    await expect.poll(() => violations.length).toBeGreaterThan(0);
    await expect(page.locator('html')).not.toHaveAttribute('data-injected', 'yes');
  });
}
