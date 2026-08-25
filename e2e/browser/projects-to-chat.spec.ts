import { expect, test } from '@playwright/test';
import { goNav, openApp } from './helpers';

test('Projects continue opens Chat with a mock session prompt', async ({ page }) => {
  await openApp(page);

  await goNav(page, 'Projects');
  await expect(page.getByRole('heading', { name: '项目' })).toBeVisible();
  await page.getByRole('tab', { name: /^Claude / }).click();

  const projectRow = page.getByRole('button', { name: /app/ }).first();
  await expect(projectRow).toBeVisible({ timeout: 20_000 });
  if ((await projectRow.getAttribute('aria-expanded')) !== 'true') {
    await projectRow.click();
  }

  await page.getByRole('button', { name: '在 Chat 继续' }).first().click();
  await expect(page).toHaveURL(/#\/chat/);
  await expect(page.getByRole('textbox', { name: '消息输入' })).toBeVisible({
    timeout: 20_000,
  });
  await expect(page.getByRole('textbox', { name: '消息输入' })).not.toHaveValue('');
});
