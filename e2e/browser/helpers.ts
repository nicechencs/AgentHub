import { expect, type Page } from '@playwright/test';

/** Placeholder only — never a real credential. */
export const MOCK_API_KEY = 'sk-ant-mock-e2e-placeholder';
export const CLAUDE_LOGIN_LABEL = 'E2E mock Claude';
export const MOCK_CWD = 'C:\\mock\\e2e-workspace';

export async function seedBrowserPrefs(page: Page): Promise<void> {
  await page.addInitScript(() => {
    window.localStorage.setItem('agenthub:onboarding-done', '1');
  });
}

export async function openApp(page: Page, hash = '/'): Promise<void> {
  await seedBrowserPrefs(page);
  const path = hash.startsWith('#') ? `/${hash}` : `/#${hash}`;
  await page.goto(path);
  await expect(page.getByRole('status', { name: 'AgentHub 正在启动' })).toBeHidden({
    timeout: 20_000,
  });
  await expect(page.getByRole('link', { name: '总览' })).toBeVisible();
  await expect(page.getByText(/功能不可用/)).toHaveCount(0);
}

export async function goNav(page: Page, name: string): Promise<void> {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  await page
    .getByRole('navigation')
    .getByRole('link', { name: new RegExp(`^${escaped}(?:$| — )`) })
    .click();
}

export async function goPath(page: Page, hash: string): Promise<void> {
  const path = hash.startsWith('#') ? `/${hash}` : `/#${hash.startsWith('/') ? hash : `/${hash}`}`;
  await page.goto(path);
}

async function waitForConnectionsReady(page: Page): Promise<void> {
  await expect(page.getByRole('heading', { name: '连接' })).toBeVisible();
  await expect(page.getByRole('button', { name: '添加授权' })).toBeVisible();
}

function loginRow(page: Page) {
  return page
    .getByRole('row')
    .filter({ hasText: CLAUDE_LOGIN_LABEL })
    .filter({ has: page.getByRole('button', { name: /切换|使用中/ }) });
}

export async function addClaudeApiKeyAndSwitch(page: Page): Promise<void> {
  await goNav(page, '连接');
  await waitForConnectionsReady(page);

  await page.getByRole('tab', { name: /^Claude / }).click();
  await page.getByRole('button', { name: '添加授权' }).click();
  await page.getByRole('menuitem', { name: '添加 API Key' }).click();

  const panel = page.locator('[data-side-inspect]');
  await expect(panel.getByRole('heading', { name: /添加 API Key/ })).toBeVisible();
  await panel.getByLabel('名称', { exact: true }).fill(CLAUDE_LOGIN_LABEL);
  const keyField = panel.getByPlaceholder('API Key').or(panel.locator('input[type="password"]'));
  await expect(keyField.first()).toBeVisible({ timeout: 20_000 });
  await keyField.first().fill(MOCK_API_KEY);
  await expect(panel.getByRole('button', { name: '添加 API Key' })).toBeEnabled();
  await panel.getByRole('button', { name: '添加 API Key' }).click();

  const row = loginRow(page);
  await expect(row).toBeVisible({ timeout: 20_000 });
  const switchBtn = row.getByRole('button', { name: '切换' });
  if (await switchBtn.isVisible()) {
    await switchBtn.click();
  }
  await expect(row.getByRole('button', { name: '使用中' })).toBeVisible({
    timeout: 20_000,
  });
}

export async function openChatComposer(page: Page): Promise<void> {
  await goNav(page, '对话');
  await expect(page.getByRole('textbox', { name: '消息输入' })).toBeVisible({
    timeout: 20_000,
  });
}

export async function setWorkingDirectory(page: Page, cwd = MOCK_CWD): Promise<void> {
  const trigger = page.getByRole('button', { name: '会话设置' });
  await trigger.click();
  const dialog = page.getByRole('dialog', { name: '会话设置' });
  await expect(dialog).toBeVisible();
  const cwdField = dialog.getByLabel('工作目录');
  await cwdField.fill(cwd);
  await cwdField.blur();
  await dialog.getByRole('button', { name: '完成' }).click();
  await expect(dialog).toBeHidden();
}
