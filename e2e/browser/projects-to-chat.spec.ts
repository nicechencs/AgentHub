import { expect, test } from '@playwright/test';
import { goNav, openApp } from './helpers';

test('Projects continue opens Chat with a mock session prompt', async ({ page }) => {
  await openApp(page);

  await goNav(page, '历史');
  await expect(page.getByRole('heading', { name: '历史' })).toBeVisible();
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

test('Projects continue with a missing cwd keeps history and offers rebind', async ({ page }) => {
  await openApp(page);

  await goNav(page, '历史');
  await expect(page.getByRole('heading', { name: '历史' })).toBeVisible();
  await page.getByRole('tab', { name: /^Claude / }).click();

  const projectRow = page.getByRole('button', { name: /^app\b/ }).first();
  await expect(projectRow).toBeVisible({ timeout: 20_000 });
  if ((await projectRow.getAttribute('aria-expanded')) !== 'true') {
    await projectRow.getByText('app', { exact: true }).click();
  }
  await expect(projectRow).toHaveAttribute('aria-expanded', 'true');

  const sessionRow = page.locator('li').filter({ hasText: '临时目录对话' }).first();
  await expect(sessionRow).toBeVisible({ timeout: 20_000 });
  await sessionRow.getByRole('button', { name: '在对话继续' }).click();

  await expect(page).toHaveURL(/#\/chat/);
  await expect(page.getByRole('textbox', { name: '消息输入' })).toBeVisible({
    timeout: 20_000,
  });
  const missing = page.locator('[data-help="chat-cwd-missing"]');
  await expect(missing).toBeVisible();
  await expect(missing.getByText('原工作目录不存在', { exact: true })).toBeVisible();
  await expect(page.getByText('在已删除的临时目录里继续改登录页', { exact: false })).toBeVisible({
    timeout: 20_000,
  });
  await expect(page.getByRole('button', { name: '改绑到项目目录' })).toBeVisible();
  await expect(page.getByRole('button', { name: '选文件夹' })).toBeVisible();
  await expect(page.getByText(/cwd is not an existing directory|invalid_arg/i)).toHaveCount(0);

  await page.getByRole('button', { name: '改绑到项目目录' }).click();
  await expect(missing).toHaveCount(0);
  await expect(page.getByText('在已删除的临时目录里继续改登录页', { exact: false })).toBeVisible();
});
