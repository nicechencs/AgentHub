import { expect, test } from '@playwright/test';
import { openApp, openChatComposer, setWorkingDirectory } from './helpers';

test('path-only file approval stays readable without inventing a diff', async ({ page }) => {
  await openApp(page);
  await openChatComposer(page);
  await setWorkingDirectory(page);

  const composerChrome = page.locator('[data-help="chat-composer"]');
  await composerChrome.getByRole('button', { name: /Claude Code/ }).click();
  const codex = page.getByRole('menuitemradio', { name: /Codex/ });
  await expect(codex).toBeVisible();
  await expect(codex).toBeEnabled();
  await codex.click();

  const composer = page.getByRole('textbox', { name: '消息输入' });
  await composer.fill('仅路径');
  await composer.press('Enter');

  const card = page.locator('[data-help="chat-file-change-preview-path-only"]');
  await expect(card).toBeVisible({ timeout: 20_000 });
  await expect(page.getByText('修改文件', { exact: true }).first()).toBeVisible();
  await expect(page.getByText('/workspace/notes.md', { exact: true })).toBeVisible();
  await expect(page.getByText('仅有路径，无内容预览')).toBeVisible();
  await expect(page.getByRole('button', { name: '允许', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '一直允许' })).toBeVisible();
  await expect(page.getByRole('button', { name: '拒绝', exact: true })).toBeVisible();
  await expect(page.getByText('@@')).toHaveCount(0);

  await page.screenshot({ path: '/opt/cursor/artifacts/file-approval-path-only.png' });
});
