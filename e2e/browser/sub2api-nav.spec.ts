import { expect, test } from '@playwright/test';
import { goNav, goPath, openApp } from './helpers';

test('Sub2API secondary nav stays hidden until Settings toggle is on; deep link still works', async ({
  page,
}) => {
  await openApp(page);
  await goNav(page, '路由');
  await expect(page).toHaveURL(/#\/routes\//);
  const routesNav = page.locator('[data-routes-nav]');
  await expect(routesNav.getByRole('link', { name: /^看板(?:$| — )/ })).toBeVisible();
  await expect(routesNav.getByRole('link', { name: /^Sub2API(?:$| — )/ })).toHaveCount(0);

  // Deep link must render even when the Settings toggle (default off) hides the nav entry.
  await goPath(page, '/routes/sub2api');
  await expect(page.getByRole('status', { name: 'AgentHub 正在启动' })).toBeHidden({
    timeout: 20_000,
  });
  await expect(page).toHaveURL(/#\/routes\/sub2api/);
  await expect(page.getByRole('heading', { name: 'Sub2API' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '尚未登录' })).toBeVisible();
  await expect(
    page.getByText('用中转站账号登录，再把 Key 同步到连接列表。'),
  ).toBeVisible();

  await goNav(page, '设置');
  const toggle = page.getByRole('switch', { name: '显示 Sub2API 页面' });
  await expect(toggle).toBeVisible();
  await expect(toggle).toHaveAttribute('aria-checked', 'false');
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-checked', 'true');

  await goNav(page, '路由');
  await expect(page.locator('[data-routes-nav]').getByRole('link', { name: /^Sub2API(?:$| — )/ })).toBeVisible();
});
