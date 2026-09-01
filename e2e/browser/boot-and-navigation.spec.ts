import { expect, test } from '@playwright/test';
import { goNav, goPath, openApp } from './helpers';

test('app boots on mock and primary navigation works', async ({ page }) => {
  await openApp(page);

  await expect(page.getByRole('heading', { name: '总览' })).toBeVisible();

  await goNav(page, '连接');
  await expect(page).toHaveURL(/#\/connections/);
  await expect(page.getByRole('heading', { name: '连接' })).toBeVisible();

  const nav = page.getByRole('navigation');
  await expect(nav.getByRole('link', { name: /^路由(?:$| — )/ })).toHaveCount(0);
  await expect(nav.getByRole('link', { name: /^插件(?:$| — )/ })).toHaveCount(0);

  await goNav(page, 'Projects');
  await expect(page).toHaveURL(/#\/projects/);
  await expect(page.getByRole('heading', { name: '项目' })).toBeVisible();

  await goNav(page, 'Chat');
  await expect(page).toHaveURL(/#\/chat/);
  await expect(
    page.getByRole('textbox', { name: '消息输入' }).or(page.getByText('还没有可对话的 Agent')),
  ).toBeVisible();
});

test('Settings tabs stay on the workbench-header left; form cards use the centered reading column; backups toolbar stays on one row', async ({ page }) => {
  await openApp(page);
  await goNav(page, '设置');
  await expect(page.getByRole('heading', { name: '设置' })).toBeVisible();
  const prefsTab = page.getByRole('tab', { name: '偏好' });
  await expect(prefsTab).toBeVisible();
  await expect(page.getByRole('heading', { name: '语言与外观' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '启动与关闭' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '侧栏' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '路由' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '技能' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '用量' })).toBeVisible();
  await expect(page.getByText('语言', { exact: true })).toBeVisible();
  const heading = page.getByRole('heading', { name: '设置' });
  const tabsList = page.getByRole('tablist').first();
  const card = page.locator('[data-card="default"]').filter({ hasText: '主题' }).first();
  await expect(card).toBeVisible();
  const headingBox = await heading.boundingBox();
  const tabListBox = await tabsList.boundingBox();
  const cardBox = await card.boundingBox();
  expect(headingBox).toBeTruthy();
  expect(tabListBox).toBeTruthy();
  expect(cardBox).toBeTruthy();
  expect(Math.abs(tabListBox!.x - headingBox!.x)).toBeLessThanOrEqual(2);
  expect(cardBox!.x).toBeGreaterThan(tabListBox!.x + 24);

  await page.getByRole('tab', { name: '本机' }).click();
  await expect(page.getByText('数据目录')).toBeVisible();
  const localCard = page.locator('[data-card="default"]').filter({ hasText: '数据目录' }).first();
  const localCardBox = await localCard.boundingBox();
  const localTabsBox = await page.getByRole('tablist').first().boundingBox();
  expect(Math.abs(localTabsBox!.x - headingBox!.x)).toBeLessThanOrEqual(2);
  expect(localCardBox!.x).toBeGreaterThan(localTabsBox!.x + 24);

  await page.getByRole('tab', { name: '备份' }).click();
  const keepCopies = page.getByText('保留本机配置副本');
  const backupBtn = page.getByRole('button', { name: '备份', exact: true });
  await expect(page.getByRole('tab', { name: /Claude/ })).toBeVisible();
  await expect(keepCopies).toBeVisible();
  await expect(backupBtn).toBeVisible();
  const pageTabs = page.getByRole('tablist').first();
  const keepBox = await keepCopies.boundingBox();
  const btnBox = await backupBtn.boundingBox();
  const pageTabsBox = await pageTabs.boundingBox();
  expect(keepBox).toBeTruthy();
  expect(btnBox).toBeTruthy();
  expect(pageTabsBox).toBeTruthy();
  expect(Math.abs(keepBox!.y + keepBox!.height / 2 - (btnBox!.y + btnBox!.height / 2))).toBeLessThanOrEqual(10);
  expect(Math.abs(keepBox!.y + keepBox!.height / 2 - (pageTabsBox!.y + pageTabsBox!.height / 2))).toBeLessThanOrEqual(10);
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

test('page title sits in the top bar with notifications; Chat has neither', async ({ page }) => {
  await openApp(page);
  const heading = page.getByRole('heading', { level: 1 }).first();
  const bell = page.getByRole('button', { name: '通知' });
  await expect(heading).toBeVisible();
  await expect(bell).toBeVisible();
  const titleBox = await heading.boundingBox();
  const bellBox = await bell.boundingBox();
  expect(titleBox).toBeTruthy();
  expect(bellBox).toBeTruthy();
  expect(titleBox!.x).toBeLessThan(bellBox!.x);
  expect(titleBox!.y).toBeLessThan(bellBox!.y + bellBox!.height);
  expect(bellBox!.y).toBeLessThan(titleBox!.y + titleBox!.height);

  await goNav(page, '连接');
  await expect(page.getByRole('heading', { name: '连接' })).toBeVisible();
  const addLogin = page.getByRole('button', { name: '添加授权' });
  const agentTabs = page.getByRole('tablist', { name: '按 Agent 筛选登录' });
  await expect(addLogin).toBeVisible();
  await expect(agentTabs).toBeVisible();
  const addBox = await addLogin.boundingBox();
  const tabsBox = await agentTabs.boundingBox();
  expect(addBox).toBeTruthy();
  expect(tabsBox).toBeTruthy();
  expect(Math.abs(addBox!.y + addBox!.height / 2 - (tabsBox!.y + tabsBox!.height / 2))).toBeLessThanOrEqual(8);
  const connectionsTop = tabsBox!.y;

  await goNav(page, 'Skills');
  const skillsTabs = page.getByRole('tab', { name: /用户技能/ });
  await expect(skillsTabs).toBeVisible();
  const skillsTop = (await skillsTabs.boundingBox())!.y;
  expect(Math.abs(skillsTop - connectionsTop)).toBeLessThanOrEqual(4);

  await goNav(page, 'MCP');
  const mcpTabs = page.getByRole('tablist').first();
  await expect(mcpTabs).toBeVisible();
  const mcpTop = (await mcpTabs.boundingBox())!.y;
  expect(Math.abs(mcpTop - connectionsTop)).toBeLessThanOrEqual(4);

  await goPath(page, '/routes/pool');
  const addOauth = page.getByRole('button', { name: 'OAuth 接入' });
  const addApi = page.getByRole('button', { name: 'API 接入' });
  const routesLead = page.getByText(/oauth 及 API 信息|孤立本机路由/);
  await expect(addOauth).toBeVisible();
  await expect(addApi).toBeVisible();
  await expect(routesLead).toBeVisible();
  const createBox = await addOauth.boundingBox();
  const leadBox = await routesLead.boundingBox();
  expect(createBox).toBeTruthy();
  expect(leadBox).toBeTruthy();
  expect(Math.abs(createBox!.y + createBox!.height / 2 - (leadBox!.y + leadBox!.height / 2))).toBeLessThanOrEqual(8);
  expect(Math.abs(leadBox!.y - connectionsTop)).toBeLessThanOrEqual(12);

  await goNav(page, 'Chat');
  await expect(page).toHaveURL(/#\/chat/);
  await expect(page.getByRole('button', { name: '通知' })).toHaveCount(0);
  await expect(page.getByRole('heading', { level: 1 })).toHaveCount(0);
});

test('new install hides Routes and Plugins until enabled in Settings', async ({ page }) => {
  await openApp(page);
  const nav = page.getByRole('navigation');
  await expect(nav.getByRole('link', { name: /^路由(?:$| — )/ })).toHaveCount(0);
  await expect(nav.getByRole('link', { name: /^插件(?:$| — )/ })).toHaveCount(0);
  await expect(nav.getByRole('link', { name: /^MCP — / })).toBeVisible();

  await goNav(page, '设置');
  await expect(page.getByRole('switch', { name: '打开路由时自动折叠' })).toBeChecked();
  await expect(page.getByRole('switch', { name: '显示路由页面' })).not.toBeChecked();
  await expect(page.getByRole('switch', { name: '显示插件页面' })).not.toBeChecked();
  await expect(
    page.getByText('显示路由页面', { exact: true }).locator('..').getByText('开发中'),
  ).toBeVisible();
  await expect(
    page.getByText('显示插件页面', { exact: true }).locator('..').getByText('开发中'),
  ).toBeVisible();

  await page.getByRole('switch', { name: '显示路由页面' }).click();
  await page.getByRole('switch', { name: '显示插件页面' }).click();
  await expect(nav.getByRole('link', { name: /^路由 — / })).toBeVisible();
  await expect(nav.getByRole('link', { name: /^插件 — / })).toBeVisible();

  await goNav(page, '路由');
  await expect(page).toHaveURL(/#\/routes\/board/);
  await expect(page.getByRole('heading', { name: '看板' })).toBeVisible();
  await expect(page.getByText('用量统计')).toBeVisible();
  await expect(page.getByRole('button', { name: '收起侧栏' })).toHaveCount(0);

  await goNav(page, '插件');
  await expect(page).toHaveURL(/#\/plugins/);
  await expect(page.getByRole('heading', { name: '插件' })).toBeVisible();
  await expect(page.locator('header').getByText('开发中')).toBeVisible();

  await goNav(page, 'MCP');
  await expect(page).toHaveURL(/#\/mcp/);
  await expect(page.getByRole('heading', { name: 'MCP' })).toBeVisible();
  await expect(page.locator('header').getByText('开发中')).toBeVisible();
});

test('Routes secondary nav appears under /routes*; URL entry does not auto-collapse', async ({ page }) => {
  await openApp(page);
  await goPath(page, '/routes');
  await expect(page).toHaveURL(/#\/routes\/board/);
  const secondary = page.locator('[data-routes-nav]');
  await expect(secondary).toBeVisible();
  await expect(secondary.getByRole('link', { name: /^路由列表/ })).toHaveCount(0);
  await expect(secondary.getByRole('link', { name: /^看板/ })).toBeVisible();
  await expect(secondary.getByRole('link', { name: /^连接池/ })).toBeVisible();
  await expect(secondary.getByRole('link', { name: /^令牌/ })).toBeVisible();
  await expect(secondary.getByRole('button', { name: '展开侧栏' })).toBeVisible();
  await expect(page.getByRole('button', { name: '收起侧栏' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '看板' })).toBeVisible();
  await expect(page.getByText('用量统计')).toBeVisible();

  await secondary.getByRole('link', { name: /^连接池/ }).click();
  await expect(page).toHaveURL(/#\/routes\/pool/);
  await expect(page.getByRole('heading', { name: '连接池' })).toBeVisible();

  await secondary.getByRole('link', { name: /^监控$/ }).click();
  await expect(page).toHaveURL(/#\/routes\/activity/);
  await expect(page.getByRole('heading', { name: '监控' })).toBeVisible();

  await goPath(page, '/');
  await expect(page).toHaveURL(/#\/$/);
  await expect(page.locator('[data-routes-nav]')).toHaveCount(0);
});
