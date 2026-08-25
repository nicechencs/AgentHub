import { expect, test } from '@playwright/test';
import { addClaudeApiKeyAndSwitch, CLAUDE_LOGIN_LABEL, openApp } from './helpers';

test('Connections can add a mock API Key and switch it into use', async ({ page }) => {
  await openApp(page, '/connections');
  await addClaudeApiKeyAndSwitch(page);

  await expect(page.getByText(CLAUDE_LOGIN_LABEL, { exact: true })).toBeVisible();
});
