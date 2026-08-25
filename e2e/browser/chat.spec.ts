import { expect, test } from '@playwright/test';
import { openApp, openChatComposer, setWorkingDirectory } from './helpers';

test('Chat sends a prompt and shows the mock reply', async ({ page }) => {
  await openApp(page);
  await openChatComposer(page);
  await setWorkingDirectory(page);

  const composer = page.getByRole('textbox', { name: '消息输入' });
  await composer.fill('e2e mock ping');
  await expect(page.getByRole('button', { name: '发送' })).toBeEnabled();
  await page.getByRole('button', { name: '发送' }).click();

  await expect(page.getByText('e2e mock ping')).toBeVisible();
  await expect(page.getByText(/模拟回复/)).toBeVisible({ timeout: 20_000 });
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
