import { expect, test, type Page } from '@playwright/test';

async function openFirstLaunch(page: Page) {
  await page.goto('/#/');
  await expect(page.getByRole('status', { name: 'AgentHub 正在启动' })).toBeHidden({
    timeout: 20_000,
  });
  const dialog = page.getByRole('dialog', { name: '欢迎使用 AgentHub' });
  await expect(dialog).toBeVisible();
  return dialog;
}

test('first launch can skip the guide and keeps default sidebar pages', async ({ page }) => {
  const dialog = await openFirstLaunch(page);
  await expect(dialog.getByRole('checkbox', { name: '本地路由' })).toBeVisible();
  await expect(dialog.getByRole('checkbox', { name: 'Sub2API 站点' })).toBeVisible();
  await expect(dialog.getByRole('button', { name: '继续' })).toBeDisabled();
  await dialog.getByRole('button', { name: '跳过引导' }).click();
  await expect(dialog).toBeHidden();

  const nav = page.getByRole('navigation');
  await expect(nav.getByRole('link', { name: /^路由(?:$| — )/ })).toBeVisible();
  await expect(nav.getByRole('link', { name: /^Sub2API(?:$| — )/ })).toHaveCount(0);
});

test('choosing only Sub2API hides Routes; Settings can show Routes again', async ({ page }) => {
  const dialog = await openFirstLaunch(page);
  await dialog.getByRole('checkbox', { name: 'Sub2API 站点' }).click();
  await expect(dialog.getByRole('button', { name: '继续' })).toBeEnabled();
  await dialog.getByRole('button', { name: '跳过引导' }).click();
  await expect(dialog).toBeHidden();

  const nav = page.getByRole('navigation');
  await expect(nav.getByRole('link', { name: /^Sub2API(?:$| — )/ })).toBeVisible();
  await expect(nav.getByRole('link', { name: /^路由(?:$| — )/ })).toHaveCount(0);

  await nav.getByRole('link', { name: /^设置(?:$| — )/ }).click();
  const routesToggle = page.getByRole('switch', { name: '显示路由页面' });
  await expect(routesToggle).not.toBeChecked();
  await expect(page.getByRole('switch', { name: '显示 Sub2API 页面' })).toBeChecked();
  await routesToggle.click();
  await expect(nav.getByRole('link', { name: /^路由(?:$| — )/ })).toBeVisible();
});

test('choosing local routing only hides Sub2API; Settings can show it again', async ({ page }) => {
  const dialog = await openFirstLaunch(page);
  await dialog.getByRole('checkbox', { name: '本地路由' }).click();
  await dialog.getByRole('button', { name: '跳过引导' }).click();
  await expect(dialog).toBeHidden();

  const nav = page.getByRole('navigation');
  await expect(nav.getByRole('link', { name: /^路由(?:$| — )/ })).toBeVisible();
  await expect(nav.getByRole('link', { name: /^Sub2API(?:$| — )/ })).toHaveCount(0);

  await nav.getByRole('link', { name: /^设置(?:$| — )/ }).click();
  const sub2apiToggle = page.getByRole('switch', { name: '显示 Sub2API 页面' });
  await expect(sub2apiToggle).not.toBeChecked();
  await sub2apiToggle.click();
  await expect(nav.getByRole('link', { name: /^Sub2API(?:$| — )/ })).toBeVisible();
});
