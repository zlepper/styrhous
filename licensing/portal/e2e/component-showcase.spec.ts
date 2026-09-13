import { expect, test, type Page } from '@playwright/test';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const MAX_BUTTON_SHOWCASE_RASTER_DIFF_PIXELS = 100;
const MAX_SELECT_SHOWCASE_RASTER_DIFF_PIXELS = 1;

async function waitForDeterministicRendering(page: Page) {
  await page.evaluate(() => document.fonts.ready);
}

async function expectNativeSurface(page: Page, testId: string, width: number, height: number) {
  const showcase = page.getByTestId(testId);
  await expect(showcase).toHaveCSS('background-position', '8px 8px');
  await expect(showcase).toHaveCSS('background-size', `${width}px ${height}px`);
}

test('renders the native-sized button showcase', async ({ page }) => {
  await page.goto('/showcase/buttons');
  await waitForDeterministicRendering(page);

  await expect(page.getByTestId('button-showcase')).toHaveCSS('width', '1536px');
  await expect(page.getByTestId('button-showcase')).toHaveCSS('height', '1024px');
  const nativeSizes = {
    xs: { width: 87.8, height: 32 },
    sm: { width: 107.8, height: 34 },
    md: { width: 123.8, height: 38 },
    lg: { width: 133.8, height: 40 },
    xl: { width: 157.8, height: 44 }
  };
  const webSizes: Record<string, { width: number; height: number }> = {};
  for (const [size, dimensions] of Object.entries(nativeSizes)) {
    const bounds = await page
      .locator(`[data-variant="primary"][data-size="${size}"]`)
      .boundingBox();
    expect(bounds).not.toBeNull();
    webSizes[size] = {
      width: Math.round(bounds!.width * 10) / 10,
      height: Math.round(bounds!.height * 10) / 10
    };
    await expect(page.locator(`[data-variant="primary"][data-size="${size}"]`)).toHaveCSS(
      'height',
      `${dimensions.height}px`
    );
  }
  expect(webSizes).toEqual(nativeSizes);
  expect(await page.locator('body').ariaSnapshot()).toMatchSnapshot(
    'button-showcase-accessibility.txt'
  );
  await expect(page).toHaveScreenshot('button-showcase.png', {
    maxDiffPixels: MAX_BUTTON_SHOWCASE_RASTER_DIFF_PIXELS
  });
});

for (const variant of ['primary', 'secondary', 'soft', 'danger'] as const) {
  test(`renders native-sized ${variant} button interaction states`, async ({ page }) => {
    await page.goto(`/showcase/button-states?variant=${variant}&state=hovered`);
    await waitForDeterministicRendering(page);
    await expectNativeSurface(page, 'button-interaction-showcase', 216, 58);

    const hovered = page.getByRole('button', { name: 'Hovered' });
    await hovered.hover();
    expect(await page.locator('body').ariaSnapshot()).toMatchSnapshot(
      `${variant}-hovered-accessibility.txt`
    );
    await expect(page).toHaveScreenshot(`${variant}-hovered.png`, {
      maxDiffPixels: 0
    });

    await page.goto(`/showcase/button-states?variant=${variant}&state=pressed`);
    await waitForDeterministicRendering(page);

    const pressed = page.getByRole('button', { name: 'Pressed' });
    const bounds = await pressed.boundingBox();
    expect(bounds).not.toBeNull();
    await page.mouse.move(bounds!.x + bounds!.width / 2, bounds!.y + bounds!.height / 2);
    await page.mouse.down();
    try {
      expect(await page.locator('body').ariaSnapshot()).toMatchSnapshot(
        `${variant}-pressed-accessibility.txt`
      );
      await expect(page).toHaveScreenshot(`${variant}-pressed.png`, {
        maxDiffPixels: 0
      });
    } finally {
      await page.mouse.up();
    }
  });

  test(`renders native-sized ${variant} button focus and disabled states`, async ({ page }) => {
    await page.goto(`/showcase/button-states?variant=${variant}&state=focused`);
    await waitForDeterministicRendering(page);

    const focused = page.getByRole('button', { name: 'Focused' });
    await focused.focus();
    await expect(focused).toBeFocused();
    expect(await page.locator('body').ariaSnapshot()).toMatchSnapshot(
      `${variant}-focused-accessibility.txt`
    );
    await expect(page).toHaveScreenshot(`${variant}-focused.png`, {
      maxDiffPixels: 0
    });

    await page.goto(`/showcase/button-states?variant=${variant}&state=disabled`);
    await waitForDeterministicRendering(page);

    const disabled = page.getByRole('button', { name: 'Disabled' });
    await expect(disabled).toBeDisabled();
    expect(await page.locator('body').ariaSnapshot()).toMatchSnapshot(
      `${variant}-disabled-accessibility.txt`
    );
    await expect(page).toHaveScreenshot(`${variant}-disabled.png`, {
      maxDiffPixels: 0
    });
  });
}

test('renders native-sized unfocused and focused text inputs', async ({ page }) => {
  await page.goto('/showcase/text-inputs');
  await waitForDeterministicRendering(page);
  await expectNativeSurface(page, 'text-input-showcase', 320, 71);

  const metadataKey = page.getByLabel('Metadata key');
  const columnHeader = page.getByLabel('Column header');
  await expect(metadataKey).toHaveCSS('height', '28px');
  await expect(metadataKey).toHaveCSS('width', '320px');
  await expect(columnHeader).toHaveValue('Application');

  expect(await page.locator('body').ariaSnapshot()).toMatchSnapshot(
    'text-inputs-unfocused-accessibility.txt'
  );
  await expect(page).toHaveScreenshot('text-inputs-unfocused.png', {
    maxDiffPixels: 0
  });

  await metadataKey.focus();
  await expect(metadataKey).toBeFocused();
  await expect(metadataKey).toHaveCSS('outline-style', 'none');
  await expect(metadataKey).toHaveCSS('box-shadow', 'none');
  expect(await page.locator('body').ariaSnapshot()).toMatchSnapshot(
    'text-inputs-focused-accessibility.txt'
  );
  await expect(page).toHaveScreenshot('text-inputs-focused.png', {
    maxDiffPixels: 0
  });
});

test('renders the production select at the native combobox size', async ({ page }) => {
  await page.goto('/showcase/comboboxes');
  await waitForDeterministicRendering(page);
  await expectNativeSurface(page, 'select-showcase', 250, 56);

  const select = page.getByRole('combobox', { name: 'Assigned to' });
  const chevron = page.locator('.select-control-chevron');
  await expect(select).toHaveCSS('height', '36px');
  await expect(select).toHaveCSS('width', '250px');
  await expect(select).toHaveValue('');
  const chevronBounds = await chevron.boundingBox();
  expect(chevronBounds).toEqual({ x: 234, y: 38, width: 16, height: 16 });

  expect(await page.locator('body').ariaSnapshot()).toMatchSnapshot(
    'select-unfocused-accessibility.txt'
  );
  await expect(page).toHaveScreenshot('select-unfocused.png', {
    maxDiffPixels: MAX_SELECT_SHOWCASE_RASTER_DIFF_PIXELS
  });

  await select.focus();
  await expect(select).toBeFocused();
  expect(await page.locator('body').ariaSnapshot()).toMatchSnapshot(
    'select-focused-accessibility.txt'
  );
  await expect(page).toHaveScreenshot('select-focused.png', {
    maxDiffPixels: MAX_SELECT_SHOWCASE_RASTER_DIFF_PIXELS
  });
});

test('keeps the decorative select chevron click-through', async ({ page }) => {
  await page.goto('/showcase/comboboxes');
  await waitForDeterministicRendering(page);

  const select = page.getByRole('combobox', { name: 'Assigned to' });
  const chevronBounds = await page.locator('.select-control-chevron').boundingBox();
  expect(chevronBounds).not.toBeNull();

  await page.mouse.click(
    chevronBounds!.x + chevronBounds!.width / 2,
    chevronBounds!.y + chevronBounds!.height / 2
  );
  await expect(select).toBeFocused();
  await page.keyboard.press('Escape');
  await select.press('ArrowDown');
  await expect(select).toHaveValue('Michael Foster');
});

test('keeps every native/web comparison tied to its reviewed source images', () => {
  const repositoryRoot = resolve(import.meta.dirname, '../../..');
  const requiredComparisonNames = [
    'button-showcase',
    'danger-hovered',
    'danger-pressed',
    'primary-hovered',
    'primary-pressed',
    'secondary-hovered',
    'secondary-pressed',
    'select-unfocused',
    'soft-hovered',
    'soft-pressed',
    'text-inputs-focused',
    'text-inputs-unfocused'
  ];
  const manifest = JSON.parse(
    readFileSync(resolve(import.meta.dirname, 'component-showcase.comparisons.json'), 'utf8')
  ) as {
    viewport: { width: number; height: number };
    comparisons: Array<{
      name: string;
      expected: string;
      actual: string;
      expectedSha256: string;
      actualSha256: string;
      rawMae: number;
      maxRawMae: number;
      region: { x: number; y: number; width: number; height: number };
    }>;
  };

  expect(manifest.comparisons.map(({ name }) => name).toSorted()).toEqual(
    requiredComparisonNames.toSorted()
  );
  for (const comparison of manifest.comparisons) {
    const expectedPath = resolve(repositoryRoot, comparison.expected);
    const actualPath = resolve(repositoryRoot, comparison.actual);
    expect(existsSync(expectedPath)).toBe(true);
    expect(existsSync(actualPath)).toBe(true);
    expect(pngDimensions(expectedPath)).toEqual(manifest.viewport);
    expect(pngDimensions(actualPath)).toEqual(manifest.viewport);
    expect(sha256(expectedPath)).toBe(comparison.expectedSha256);
    expect(sha256(actualPath)).toBe(comparison.actualSha256);
    expect(comparison.rawMae).toBeGreaterThanOrEqual(0);
    expect(comparison.maxRawMae).toBeGreaterThan(0);
    expect(comparison.maxRawMae).toBeLessThanOrEqual(0.06);
    expect(comparison.rawMae).toBeLessThanOrEqual(comparison.maxRawMae);
    expect(comparison.maxRawMae - comparison.rawMae).toBeLessThanOrEqual(0.002);
    expect(comparison.region.x).toBeGreaterThanOrEqual(0);
    expect(comparison.region.y).toBeGreaterThanOrEqual(0);
    expect(comparison.region.width).toBeGreaterThan(0);
    expect(comparison.region.height).toBeGreaterThan(0);
    expect(comparison.region.x + comparison.region.width).toBeLessThanOrEqual(
      manifest.viewport.width
    );
    expect(comparison.region.y + comparison.region.height).toBeLessThanOrEqual(
      manifest.viewport.height
    );
  }
});

function sha256(path: string): string {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function pngDimensions(path: string): { width: number; height: number } {
  const bytes = readFileSync(path);
  if (bytes.length < 24 || bytes.subarray(1, 4).toString('ascii') !== 'PNG') {
    throw new Error(`${path} is not a valid PNG`);
  }

  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
}
