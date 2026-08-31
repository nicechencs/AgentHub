import { expect, test } from '@playwright/test';
import { goPath, openApp } from './helpers';

test('routes board grouping stays independent of filters unless a dimension is already narrowed', async ({ page }) => {
  await openApp(page);
  await goPath(page, '/routes/board');
  await expect(page.getByRole('heading', { name: '路由看板' })).toBeVisible();
  await expect(page.getByText('用量统计')).toBeVisible();
  await expect(page.getByLabel('按本机入口筛选用量')).toBeVisible({ timeout: 20_000 });
  await expect(page.getByLabel('用量分组')).toBeVisible();
  await expect(page.getByText('请求次数')).toBeVisible();
  await expect(page.getByText('输入(7 天)')).toBeVisible();
  await expect(page.getByText('7 天 Token 用量')).toBeVisible();
  await expect(page.getByRole('heading', { name: '按接口' })).toBeVisible();
  await expect(page.locator('.recharts-surface')).toBeVisible();

  await page.getByRole('tab', { name: '模型' }).click();
  await expect(page.getByRole('heading', { name: '按模型' })).toBeVisible({ timeout: 15_000 });

  await page.getByRole('tab', { name: '接口' }).click();
  await expect(page.getByRole('heading', { name: '按接口' })).toBeVisible({ timeout: 15_000 });

  await page.getByLabel('按接口筛选用量').click();
  await page.getByRole('option', { name: '回复接口' }).click();
  await expect(page.getByRole('tab', { name: '接口' })).toBeDisabled();
  await expect(page.getByRole('heading', { name: '按模型' })).toBeVisible({ timeout: 15_000 });

  await page.getByLabel('统计时间范围').click();
  await page.getByRole('option', { name: '今天' }).click();
  await expect(page.getByText('输入(今天)')).toBeVisible({ timeout: 15_000 });

  await page.getByRole('link', { name: '打开监控日志' }).click();
  await expect(page).toHaveURL(/#\/routes\/activity/);
});
