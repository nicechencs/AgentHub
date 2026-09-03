import { expect, test, type Page } from '@playwright/test';
import { goNav, openApp } from './helpers';

async function usageCombobox(page: Page, selected: string) {
  return page.getByRole('combobox').filter({ hasText: selected });
}

async function chooseOption(page: Page, selected: string, option: string) {
  const trigger = await usageCombobox(page, selected);
  await expect(trigger).toBeVisible();
  await trigger.click();
  await page.getByRole('option', { name: option, exact: true }).click();
}

test('Dashboard usage filters stay selected after leaving and returning', async ({ page }) => {
  await openApp(page);
  await expect(page.getByRole('heading', { name: '总览' })).toBeVisible();
  await expect(page.getByText(/Token 用量/)).toBeVisible({ timeout: 20_000 });
  await expect(await usageCombobox(page, '全部 Agent')).toBeVisible();
  await expect(await usageCombobox(page, '全部模型')).toBeVisible();
  const sevenDays = page.getByRole('tab', { name: '7 天', exact: true });
  await expect(sevenDays).toBeVisible();
  await expect(sevenDays).toHaveAttribute('aria-selected', 'true');

  await page.getByRole('tab', { name: '今天', exact: true }).click();
  await chooseOption(page, '全部 Agent', 'Claude Code');
  await expect(page.getByText(/Token 用量/)).toBeVisible({ timeout: 20_000 });

  const modelTrigger = await usageCombobox(page, '全部模型');
  await modelTrigger.click();
  const modelOption = page.getByRole('option').filter({ hasNotText: '全部模型' }).first();
  await expect(modelOption).toBeVisible();
  const modelName = (await modelOption.innerText()).trim();
  expect(modelName.length).toBeGreaterThan(0);
  await modelOption.click();

  await expect(await usageCombobox(page, 'Claude Code')).toBeVisible();
  await expect(await usageCombobox(page, modelName)).toBeVisible();
  await expect(page.getByRole('tab', { name: '今天', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );

  await goNav(page, '连接');
  await expect(page.getByRole('heading', { name: '连接' })).toBeVisible();

  await goNav(page, '总览');
  await expect(page.getByRole('heading', { name: '总览' })).toBeVisible();
  await expect(await usageCombobox(page, 'Claude Code')).toBeVisible();
  await expect(await usageCombobox(page, modelName)).toBeVisible();
  await expect(page.getByRole('tab', { name: '今天', exact: true })).toHaveAttribute(
    'aria-selected',
    'true',
  );
});
