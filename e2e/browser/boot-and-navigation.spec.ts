import { expect, test } from '@playwright/test';
import { goNav, openApp } from './helpers';

test('app boots on mock and primary navigation works', async ({ page }) => {
  await openApp(page);

  await expect(page.getByRole('heading', { name: '总览' })).toBeVisible();

  await goNav(page, '连接');
  await expect(page).toHaveURL(/#\/connections/);
  await expect(page.getByRole('heading', { name: '连接' })).toBeVisible();

  await goNav(page, '路由');
  await expect(page).toHaveURL(/#\/routes/);
  await expect(page.getByRole('heading', { name: '路由' })).toBeVisible();

  await goNav(page, 'Projects');
  await expect(page).toHaveURL(/#\/projects/);
  await expect(page.getByRole('heading', { name: '项目' })).toBeVisible();

  await goNav(page, 'Chat');
  await expect(page).toHaveURL(/#\/chat/);
  await expect(
    page.getByRole('textbox', { name: '消息输入' }).or(page.getByText('还没有可对话的 Agent')),
  ).toBeVisible();
});

async function pageTitleBox(page: Parameters<typeof goNav>[0]) {
  const heading = page.getByRole('heading', { level: 1 }).first();
  await expect(heading).toBeVisible();
  const box = await heading.boundingBox();
  expect(box).toBeTruthy();
  return { x: box!.x, y: box!.y };
}

test('Agents page title shares the same top-left inset as other pages', async ({ page }) => {
  await openApp(page);
  const dashboard = await pageTitleBox(page);

  await goNav(page, 'Agents');
  await expect(page).toHaveURL(/#\/agents/);
  await expect(page.getByRole('heading', { name: 'Agent 管理' })).toBeVisible();
  const agents = await pageTitleBox(page);

  await goNav(page, 'Skills');
  await expect(page).toHaveURL(/#\/skills/);
  const skills = await pageTitleBox(page);

  await goNav(page, '连接');
  await expect(page).toHaveURL(/#\/connections/);
  const connections = await pageTitleBox(page);

  for (const box of [agents, skills, connections]) {
    expect(Math.abs(box.x - dashboard.x)).toBeLessThanOrEqual(1);
    expect(Math.abs(box.y - dashboard.y)).toBeLessThanOrEqual(1);
  }
});
