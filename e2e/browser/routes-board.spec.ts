import { expect, test } from '@playwright/test';
import { goPath, openApp } from './helpers';

test('routes board follows dashboard layout and local-route usage', async ({ page }) => {
  await openApp(page);
  await goPath(page, '/routes/board');
  await expect(page.getByRole('heading', { name: '看板' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Messages' })).toBeVisible({ timeout: 20_000 });
  await expect(page.getByRole('button', { name: 'Responses · Codex' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Responses · Grok' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Chat Completions' })).toBeVisible();
  await expect(page.getByText(/共有 \d+ 个入口 Key/)).toBeVisible();
  await expect(page.getByText('用量统计')).toBeVisible();
  await expect(page.getByLabel('按本机转发筛选用量')).toBeVisible({ timeout: 20_000 });
  await expect(page.getByText('请求次数')).toBeVisible();
  await expect(page.getByText('输入(7 天)')).toBeVisible();
  await expect(page.getByText('7 天 Token 用量')).toBeVisible();
  await expect(page.getByRole('heading', { name: '按接口' })).toBeVisible();
  await expect(page.locator('.recharts-surface')).toBeVisible();
  await expect(page.getByRole('tab', { name: '模型' })).toHaveCount(0);

  await page.getByRole('button', { name: 'Responses · Codex' }).click();
  await expect(page.getByText(/当前端点类型支持 /)).toBeVisible();
  await expect(page.getByRole('heading', { name: '按模型' })).toBeVisible({ timeout: 15_000 });

  await page.getByRole('tab', { name: '今天' }).click();
  await expect(page.getByText('输入(今天)')).toBeVisible({ timeout: 15_000 });

  await page.getByRole('link', { name: '打开监控' }).click();
  await expect(page).toHaveURL(/#\/routes\/activity/);
});
