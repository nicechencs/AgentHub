import { expect, test } from '@playwright/test';
import { openApp, openChatComposer, setWorkingDirectory } from './helpers';

test('file approval card shows protocol preview and an honest empty state', async ({ page }) => {
  await openApp(page);
  await openChatComposer(page);
  await setWorkingDirectory(page);

  const composer = page.getByRole('textbox', { name: '消息输入' });
  await composer.fill('修改文件');
  await page.getByRole('button', { name: '发送' }).click();

  const preview = page.locator('[data-help="chat-file-change-preview"]');
  await expect(preview).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText('修改文件').first()).toBeVisible();
  await expect(preview.getByText('/workspace/qa-codex-filechange-scratch/probe.txt')).toBeVisible();
  await expect(preview.getByText('FILECHANGE_OK')).toBeVisible();
  await expect(preview.getByText('新增')).toBeVisible();
  await expect(page.locator('[data-help="chat-file-change-preview-empty"]')).toHaveCount(0);
  await expect(page.getByRole('button', { name: '允许', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '拒绝' })).toBeVisible();
  await page.screenshot({
    path: '/opt/cursor/artifacts/file_change_preview_present.png',
    fullPage: true,
  });

  await page.getByRole('button', { name: '拒绝' }).click();
  await expect(page.locator('[data-help="chat-file-change-preview"]')).toHaveCount(0);
  await expect(page.getByRole('button', { name: '停止', exact: true })).toHaveCount(0, { timeout: 15_000 });

  await page.getByRole('button', { name: '新建对话' }).click();
  await setWorkingDirectory(page);
  const nextComposer = page.getByRole('textbox', { name: '消息输入' });
  await nextComposer.fill('path only');
  await page.getByRole('button', { name: '发送' }).click();

  const empty = page.locator('[data-help="chat-file-change-preview-empty"]');
  await expect(empty).toBeVisible({ timeout: 10_000 });
  await expect(empty.getByText('/workspace/notes.md')).toBeVisible();
  await expect(empty.getByText('暂无改动预览')).toBeVisible();
  await expect(empty.getByText('@@')).toHaveCount(0);
  await page.screenshot({
    path: '/opt/cursor/artifacts/file_change_preview_empty.png',
    fullPage: true,
  });
});
