import { expect, test } from '@playwright/test';
import { openApp, openChatComposer, setWorkingDirectory } from './helpers';

test('empty chat starter card fills the composer without sending', async ({ page }) => {
  await openApp(page);
  await openChatComposer(page);
  await setWorkingDirectory(page);

  await expect(page.getByText('开始对话')).toBeVisible();
  await expect(page.getByText(/发送第一条消息/)).toHaveCount(0);
  await expect(page.getByText('示例只填入输入框，由你发送')).toHaveCount(0);

  const composer = page.getByRole('textbox', { name: '消息输入' });
  await expect(composer).toHaveAttribute('placeholder', /发消息|发给 Agent|Send a message/);
  await expect(composer).not.toHaveAttribute('placeholder', /不能中途补充/);

  const card = page.getByRole('button', { name: '了解这个项目' });
  await expect(card).toBeVisible();
  await page.screenshot({ path: '/opt/cursor/artifacts/chat_empty_invite.png' });
  await card.click();

  await expect(composer).toHaveValue('请帮我了解这个项目的结构和主要功能。');
  await expect(composer).toBeFocused();
  await expect(page.getByRole('log')).not.toContainText('请帮我了解这个项目的结构和主要功能。');
  await expect(page.locator('[data-help="chat-composer-hint"]')).toHaveCount(0);
  await expect(composer).toHaveAttribute('title', /不能中途补充/);
  await page.screenshot({ path: '/opt/cursor/artifacts/chat_chip_fills_draft.png' });
});

test('Enter sends and Shift+Enter inserts a newline without sending', async ({ page }) => {
  await openApp(page);
  await openChatComposer(page);
  await setWorkingDirectory(page);

  const composer = page.getByRole('textbox', { name: '消息输入' });
  await composer.click();
  await composer.fill('first line');
  await page.keyboard.press('Shift+Enter');
  await page.keyboard.type('second line');
  await expect(composer).toHaveValue('first line\nsecond line');
  await expect(page.getByRole('log').getByText('first line')).toHaveCount(0);

  await page.keyboard.press('Enter');
  await expect(page.getByRole('log').getByText('first line')).toBeVisible({ timeout: 20_000 });
  await expect(page.getByRole('log').getByText(/模拟回复/)).toBeVisible({ timeout: 20_000 });
});

test('Chat sends a prompt and shows the mock reply', async ({ page }) => {
  await openApp(page);
  await openChatComposer(page);
  await setWorkingDirectory(page);

  const composer = page.getByRole('textbox', { name: '消息输入' });
  await expect(page.getByText('Enter 发送 · Shift+Enter 换行')).toHaveCount(0);
  await expect(composer).toHaveAttribute('title', /Enter 发送/);
  await expect(page.getByRole('button', { name: '发送' })).toBeDisabled();
  await composer.fill('e2e mock ping');
  await expect(page.getByRole('button', { name: '发送' })).toBeEnabled();
  await page.getByRole('button', { name: '发送' }).click();
  await expect(composer).toBeFocused();
  await expect(page.getByRole('button', { name: '停止', exact: true })).toBeVisible();
  await expect(page.getByText('Enter 排队 · Shift+Enter 换行')).toBeVisible();
  await composer.fill('下一句');
  await composer.press('Enter');
  const queue = page.locator('[data-help="chat-queued-follow-ups"]');
  await expect(queue.getByText('已排队 1 条 · 本轮结束后发送')).toBeVisible();
  await expect(queue.getByText('下一句', { exact: true })).toBeVisible();
  await expect(queue.getByRole('button', { name: '取消这条' })).toBeVisible();
  await expect(page.getByText('下一句；')).toHaveCount(0);
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

test('model menu uses readable names and Ctrl+Shift+I opens it', async ({ page }) => {
  await openApp(page);
  await openChatComposer(page);
  await setWorkingDirectory(page);

  const composerChrome = page.locator('[data-help="chat-composer"]');
  await composerChrome.getByRole('button', { name: /Claude Code/ }).click();
  const codex = page.getByRole('menuitemradio', { name: /Codex/ });
  await expect(codex).toBeVisible();
  await expect(codex).toBeEnabled();
  await codex.click();

  const modelTrigger = page.locator('[data-help="chat-model"]');
  await expect(modelTrigger).toBeVisible({ timeout: 20_000 });
  await expect(modelTrigger).not.toHaveText(/gpt-[0-9]|grok-[0-9]|claude-/i);

  await modelTrigger.click();
  await expect(page.getByRole('menuitemradio', { name: 'GPT Mock' })).toBeVisible();
  const spark = page.getByRole('menuitemradio', { name: 'GPT 5.3 Codex Spark' });
  await expect(spark).toBeVisible();
  await spark.click();
  await expect(modelTrigger).toHaveText('GPT 5.3 Codex Spark');

  const effortTrigger = page.locator('[data-help="chat-effort"]');
  await expect(effortTrigger).toBeEnabled();
  await expect(page.getByText('可能更慢')).toBeVisible();
  await effortTrigger.click();
  await expect(page.getByRole('menuitemradio', { name: /低/ })).toBeVisible();
  await expect(page.getByText('更快')).toBeVisible();
  await page.keyboard.press('Escape');

  await page.evaluate(() => {
    const target = document.activeElement ?? document.body;
    target.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'I',
        code: 'KeyI',
        ctrlKey: true,
        shiftKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );
  });
  await expect(page.getByRole('menuitemradio', { name: 'GPT 5.3 Codex Spark' })).toBeVisible();
});

test('Stop stays 正在停止 until the mock turn ends', async ({ page }) => {
  await openApp(page);
  await openChatComposer(page);
  await setWorkingDirectory(page);

  const composer = page.getByRole('textbox', { name: '消息输入' });
  await composer.fill('e2e mock stop');
  await page.getByRole('button', { name: '发送' }).click();
  const stop = page.locator('[data-help="chat-stop"]');
  await expect(stop).toBeVisible();
  await stop.click();
  await expect(page.getByRole('button', { name: '停止', exact: true })).toHaveCount(0);
  await expect(page.getByText('已按你的要求停止。可恢复草稿后重发。')).toBeVisible({
    timeout: 10_000,
  });
  await expect(page.getByRole('main').getByText('已停止', { exact: true })).toBeVisible();
});

test('shortcut overview opens from the composer and lists new-chat keys', async ({ page }) => {
  await openApp(page);
  await openChatComposer(page);

  const trigger = page.getByRole('button', { name: '快捷键' });
  await expect(trigger).toHaveAttribute('aria-expanded', 'false');
  await trigger.hover();
  const panel = page.locator('[data-help="chat-shortcuts-popover"]');
  await expect(panel).toBeVisible();
  await expect(trigger).toHaveAttribute('aria-expanded', 'true');
  await expect(panel.getByText('组字时 Enter 不发送')).toBeVisible();
  await expect(panel.getByText('新建对话')).toBeVisible();
  await expect(panel.getByText('Ctrl+N')).toBeVisible();
  await expect(panel.getByText('换模型')).toBeVisible();

  await page.getByRole('button', { name: '会话设置' }).hover();
  await expect(panel).toBeHidden();

  await trigger.click();
  await expect(panel).toBeVisible();
  await expect(trigger).toHaveAttribute('aria-expanded', 'true');
  await page.screenshot({
    path: '/opt/cursor/artifacts/chat_shortcut_overview.png',
    fullPage: true,
  });
  await page.keyboard.press('Escape');
  await expect(panel).toBeHidden();
  await expect(trigger).toHaveAttribute('aria-expanded', 'false');

  await page.getByRole('button', { name: '会话设置' }).focus();
  await page.evaluate(() => {
    const target = document.activeElement ?? document.body;
    target.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: '?',
        bubbles: true,
        cancelable: true,
      }),
    );
  });
  await expect(page.getByRole('dialog', { name: '快捷键' })).toBeVisible();
  await page.getByRole('dialog', { name: '快捷键' }).getByRole('button', { name: '关闭' }).click();
  await expect(page.getByRole('dialog', { name: '快捷键' })).toBeHidden();

  await page.getByRole('button', { name: '会话设置' }).focus();
  await page.evaluate(() => {
    const target = document.activeElement ?? document.body;
    target.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: '/',
        code: 'Slash',
        shiftKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );
  });
  await expect(page.getByRole('dialog', { name: '快捷键' })).toBeVisible();
  await page.getByRole('dialog', { name: '快捷键' }).getByRole('button', { name: '关闭' }).click();

  const composer = page.getByRole('textbox', { name: '消息输入' });
  await composer.click();
  await composer.fill('');
  await page.keyboard.type('?');
  await expect(composer).toHaveValue('?');
  await expect(page.getByRole('dialog', { name: '快捷键' })).toHaveCount(0);
});

test('Ctrl+N starts a new chat', async ({ page }) => {
  await openApp(page);
  await openChatComposer(page);

  const composer = page.getByRole('textbox', { name: '消息输入' });
  await composer.fill('keep this draft');
  await composer.evaluate((el) => {
    el.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'n',
        code: 'KeyN',
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );
  });
  await expect(composer).toHaveValue('');
  await page.screenshot({
    path: '/opt/cursor/artifacts/chat_new_chat_ctrl_n.png',
    fullPage: true,
  });
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
