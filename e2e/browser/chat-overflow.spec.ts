import { expect, test } from '@playwright/test';
import { openApp, openChatComposer, setWorkingDirectory } from './helpers';

test('selected Agent has no duplicate overflow menu; other entries stay', async ({ page }) => {
  await openApp(page);
  await openChatComposer(page);
  await setWorkingDirectory(page);

  const composer = page.locator('[data-help="chat-composer"]');
  await expect(composer.getByRole('button', { name: '更多操作' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '更多操作' })).toHaveCount(0);

  await expect(page.getByRole('button', { name: '新建对话' })).toBeVisible();
  await expect(page.getByLabel('搜索标题或工作目录')).toBeVisible();
  await expect(page.getByRole('button', { name: '会话设置' })).toBeVisible();

  await composer.getByRole('button', { name: /Claude Code/ }).click();
  const codex = page.getByRole('menuitemradio', { name: /Codex/ });
  await expect(codex).toBeEnabled();
  await codex.click();
  await expect(page.locator('[data-help="chat-model"]')).toBeVisible({ timeout: 20_000 });
  await expect(composer.getByRole('button', { name: '更多操作' })).toHaveCount(0);
  await page.screenshot({ path: '/opt/cursor/artifacts/chat_toolbar_no_overflow.png' });

  const input = page.getByRole('textbox', { name: '消息输入' });
  await input.click();
  await input.fill('/');
  const palette = page.getByRole('listbox', { name: '更多操作' });
  await expect(palette).toBeVisible();
  await expect(palette.getByRole('option', { name: /新建对话/ })).toBeVisible();
  await expect(palette.getByRole('option', { name: /复制最近回复/ })).toBeVisible();
  await expect(palette.getByRole('option', { name: /打开历史/ })).toHaveCount(0);
  await expect(palette.getByRole('option', { name: /搜索历史会话/ })).toHaveCount(0);
  await expect(palette.getByRole('option', { name: /打开设置/ })).toHaveCount(0);
  await expect(palette.getByRole('option', { name: /打开 Agent/ })).toHaveCount(0);
  await expect(palette.getByRole('option', { name: /打开连接/ })).toHaveCount(0);
  await page.screenshot({ path: '/opt/cursor/artifacts/chat_no_agent_overflow.png' });
});
