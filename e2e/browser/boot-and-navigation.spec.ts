import { expect, test } from '@playwright/test';
import { goNav, openApp } from './helpers';

test('app boots on mock and primary navigation works', async ({ page }) => {
  await openApp(page);

  await expect(page.getByRole('heading', { name: '总览' })).toBeVisible();

  await goNav(page, '连接');
  await expect(page).toHaveURL(/#\/connections/);
  await expect(page.getByRole('heading', { name: '连接' })).toBeVisible();

  await goNav(page, '路由');
  await expect(page).toHaveURL(/#\/routes/);
  await expect(page.getByRole('heading', { name: '路由' })).toBeVisible();

  await goNav(page, 'Projects');
  await expect(page).toHaveURL(/#\/projects/);
  await expect(page.getByRole('heading', { name: '项目' })).toBeVisible();

  await goNav(page, 'Chat');
  await expect(page).toHaveURL(/#\/chat/);
  await expect(
    page.getByRole('textbox', { name: '消息输入' }).or(page.getByText('还没有可对话的 Agent')),
  ).toBeVisible();
});
