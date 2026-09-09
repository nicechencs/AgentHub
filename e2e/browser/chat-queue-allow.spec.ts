import { expect, test } from '@playwright/test';
import { openApp, openChatComposer, setWorkingDirectory } from './helpers';

async function selectComposerAgent(page: import('@playwright/test').Page, name: RegExp) {
  const composer = page.locator('[data-help="chat-composer"]');
  await composer.getByRole('button', { name: /Claude|Grok|Codex|Kiro/ }).first().click();
  const option = page.getByRole('menuitemradio', { name });
  await expect(option).toBeVisible();
  await option.click();
}

test('queued follow-ups are separate cancelable rows on the Claude queue path', async ({ page }) => {
  await openApp(page);
  await openChatComposer(page);
  await setWorkingDirectory(page);

  const composer = page.getByRole('textbox', { name: '消息输入' });
  await composer.fill('需要确认 列出目录');
  await page.getByRole('button', { name: '发送' }).click();

  const card = page.locator('[data-help="chat-allow-always"]');
  await expect(card).toBeVisible({ timeout: 10_000 });
  await expect(card.getByRole('button', { name: '一直允许' })).toBeVisible();
  await expect(card.getByText('仅当前这次进程，不保存')).toBeVisible();
  await expect(page.getByText('通常只记到本轮')).toHaveCount(0);
  await page.screenshot({
    path: '/opt/cursor/artifacts/claude_allow_always_scope.png',
    fullPage: true,
  });

  await composer.fill('第二条补充');
  await composer.press('Enter');
  await composer.fill('第三条补充');
  await composer.press('Enter');

  const queue = page.locator('[data-help="chat-queued-follow-ups"]');
  await expect(queue.getByText('已排队 2 条 · 本轮结束后发送')).toBeVisible();
  await expect(queue.getByText('第二条补充', { exact: true })).toBeVisible();
  await expect(queue.getByText('第三条补充', { exact: true })).toBeVisible();
  await expect(page.getByText('第二条补充；第三条补充')).toHaveCount(0);
  await expect(queue.getByRole('button', { name: '取消这条' })).toHaveCount(2);
  await expect(queue.getByRole('button', { name: '全部取消' })).toBeVisible();
  await page.screenshot({
    path: '/opt/cursor/artifacts/claude_queued_follow_up_list.png',
    fullPage: true,
  });

  await queue.getByRole('button', { name: '取消这条' }).first().click();
  await expect(queue.getByText('第二条补充', { exact: true })).toHaveCount(0);
  await expect(queue.getByText('第三条补充', { exact: true })).toBeVisible();
  await expect(queue.getByText('已排队 1 条 · 本轮结束后发送')).toBeVisible();
  await page.screenshot({
    path: '/opt/cursor/artifacts/claude_queued_follow_up_after_cancel_one.png',
    fullPage: true,
  });

  await queue.getByRole('button', { name: '全部取消' }).click();
  await expect(page.locator('[data-help="chat-queued-follow-ups"]')).toHaveCount(0);
  await page.screenshot({
    path: '/opt/cursor/artifacts/claude_queued_follow_up_after_cancel_all.png',
    fullPage: true,
  });
});

test('Codex Always allow names this process and usually this turn', async ({ page }) => {
  await openApp(page);
  await openChatComposer(page);
  await setWorkingDirectory(page);
  await selectComposerAgent(page, /Codex/);

  const composer = page.getByRole('textbox', { name: '消息输入' });
  await composer.fill('需要确认 改文件');
  await page.getByRole('button', { name: '发送' }).click();

  const card = page.locator('[data-help="chat-allow-always"]');
  await expect(card).toBeVisible({ timeout: 10_000 });
  await expect(card.getByRole('button', { name: '一直允许' })).toBeVisible();
  await expect(card.getByText('仅当前这次进程，通常只记到本轮，不保存')).toBeVisible();
  await page.screenshot({
    path: '/opt/cursor/artifacts/codex_allow_always_scope.png',
    fullPage: true,
  });
});
