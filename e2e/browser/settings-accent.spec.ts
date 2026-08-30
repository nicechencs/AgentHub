import { expect, test } from '@playwright/test';
import { goNav, openApp } from './helpers';

async function sidebarLogoFill(page: Parameters<typeof goNav>[0]) {
  const logo = page.locator('[data-app-logo] rect').first();
  await expect(logo).toBeVisible();
  return logo.evaluate((el) => getComputedStyle(el).fill);
}

test('Settings brand-color swatches retint the top-left app mark', async ({ page }) => {
  await openApp(page);
  await goNav(page, '设置');
  await expect(page.getByRole('heading', { name: '设置' })).toBeVisible();
  await expect(page.getByText('主色', { exact: true })).toBeVisible();

  const group = page.getByRole('radiogroup', { name: '主色' });
  const purple = group.getByRole('radio', { name: '紫色' });
  const blue = group.getByRole('radio', { name: '蓝色' });
  await expect(purple).toHaveAttribute('aria-checked', 'true');
  await expect.poll(() => sidebarLogoFill(page)).toBe('rgb(79, 70, 229)');

  await blue.click();
  await expect(blue).toHaveAttribute('aria-checked', 'true');
  await expect.poll(async () =>
    page.evaluate(() => document.documentElement.dataset.accent),
  ).toBe('blue');
  await expect.poll(() => sidebarLogoFill(page)).toBe('rgb(37, 99, 235)');

  await purple.click();
  await expect.poll(() => sidebarLogoFill(page)).toBe('rgb(79, 70, 229)');
});
