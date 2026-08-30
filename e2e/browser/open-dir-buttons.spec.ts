import { expect, test, type Locator } from '@playwright/test';
import { openApp } from './helpers';

async function expectLabeledDirButtons(root: Locator) {
  const buttons = root.getByRole('button').filter({ hasText: /^目录$/ });
  await expect(buttons.first()).toBeVisible();
  for (const btn of await buttons.all()) {
    await expect(btn).toHaveText('目录');
    await expect(btn).not.toHaveText(/打开/);
  }
}

test('Agents detail open-directory buttons show 目录', async ({ page }) => {
  await openApp(page, '/agents');
  await expect(page.getByRole('heading', { name: 'Agent 管理' })).toBeVisible();
  await page.locator('main').getByText('Claude Code', { exact: true }).click();

  const panel = page.locator('[data-side-inspect]');
  await expect(panel).toBeVisible();
  await expectLabeledDirButtons(panel);
  await expect(panel.getByRole('button', { name: '打开安装目录' }).first()).toBeVisible();
  await expect(panel.getByRole('button', { name: '打开该 Agent 的配置目录' })).toBeVisible();
});

test('Settings local open-directory buttons show 目录', async ({ page }) => {
  await openApp(page, '/settings?tab=local');
  await expect(page.getByRole('tab', { name: '本机' })).toBeVisible();
  await expect(page.getByText('数据目录', { exact: true })).toBeVisible();
  await expectLabeledDirButtons(page.locator('main'));
});
