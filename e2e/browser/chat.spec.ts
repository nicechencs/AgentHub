import { expect, test } from '@playwright/test';
import { openApp, openChatComposer, setWorkingDirectory } from './helpers';

test('empty chat starter card fills the composer without sending', async ({ page }) => {
  await openApp(page);
  await openChatComposer(page);
  await setWorkingDirectory(page);

  const card = page.getByRole('button', { name: '了解这个项目' });
  await expect(card).toBeVisible();
  await card.click();

  const composer = page.getByRole('textbox', { name: '消息输入' });
  await expect(composer).toHaveValue('请帮我了解这个项目的结构和主要功能。');
  await expect(page.getByRole('log')).not.toContainText('请帮我了解这个项目的结构和主要功能。');
});

test('Chat sends a prompt and shows the mock reply', async ({ page }) => {
  await openApp(page);
  await openChatComposer(page);
  await setWorkingDirectory(page);

  const composer = page.getByRole('textbox', { name: '消息输入' });
  await expect(page.getByText('Enter 发送 · Shift+Enter 换行')).toBeVisible();
  await expect(page.getByRole('button', { name: '发送' })).toBeDisabled();
  await composer.fill('e2e mock ping');
  await expect(page.getByRole('button', { name: '发送' })).toBeEnabled();
  await page.getByRole('button', { name: '发送' }).click();
  await expect(composer).toBeFocused();
  await expect(page.getByRole('button', { name: '停止', exact: true })).toBeVisible();
  await expect(page.getByText('Enter 排队 · Shift+Enter 换行')).toBeVisible();
  await composer.fill('下一句');
  await composer.press('Enter');
  await expect(page.getByText('已排队 1 条 · 本轮结束后发送：下一句')).toBeVisible();
  await expect(composer).toBeFocused();
  await expect(composer).toHaveValue('');

  await expect(page.getByRole('log').getByText('e2e mock ping')).toBeVisible();
  await expect(page.getByRole('log').getByText(/模拟回复/)).toBeVisible({ timeout: 20_000 });
});

test('Shift+Enter inserts a new line without sending', async ({ page }) => {
  await openApp(page);
  await openChatComposer(page);
  await setWorkingDirectory(page);

  const composer = page.getByRole('textbox', { name: '消息输入' });
  await composer.fill('第一行');
  await composer.focus();
  await page.keyboard.press('Shift+Enter');
  await page.keyboard.type('第二行');
  await expect(composer).toHaveValue('第一行\n第二行');
  await expect(page.getByRole('log')).not.toContainText('第一行');
});

test('Chat settings dialog traps Tab and restores focus after Escape', async ({ page }) => {
  await openApp(page);
  await openChatComposer(page);

  const trigger = page.getByRole('button', { name: '会话设置' });
  await trigger.click();

  const dialog = page.getByRole('dialog', { name: '会话设置' });
  await expect(dialog).toBeVisible();

  await page.keyboard.press('Tab');
  await expect(dialog.locator(':focus')).toBeVisible();

  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(trigger).toBeVisible();
  await trigger.click();
  await expect(dialog).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
});
