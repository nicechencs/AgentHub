import { expect, test } from '@playwright/test';
import { goNav, openApp } from './helpers';

test('Projects continue opens Chat with a mock session prompt', async ({ page }) => {
  await openApp(page);

  await goNav(page, '项目');
  await expect(page.getByRole('heading', { name: '项目' })).toBeVisible();
  await page.getByRole('tab', { name: /^Claude / }).click();

  const projectRow = page.getByRole('button', { name: /^app\b/ }).first();
  await expect(projectRow).toBeVisible({ timeout: 20_000 });
  if ((await projectRow.getAttribute('aria-expanded')) !== 'true') {
    // Nested path/hide controls sit in the middle of the row; click the title.
    await projectRow.getByText('app', { exact: true }).click();
  }
  await expect(projectRow).toHaveAttribute('aria-expanded', 'true');

  const continueBtn = page.getByRole('button', { name: '在对话继续' }).first();
  await expect(continueBtn).toBeVisible({ timeout: 20_000 });
  await continueBtn.click();
  await expect(page).toHaveURL(/#\/chat/);
  await expect(page.getByRole('textbox', { name: '消息输入' })).toBeVisible({
    timeout: 20_000,
  });
  await expect(page.getByText('修复登录页 token 过期问题', { exact: false }).first()).toBeVisible({
    timeout: 20_000,
  });
});
